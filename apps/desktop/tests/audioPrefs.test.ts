import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { test } from "node:test";

const audioPrefs = await import("../src/audioPrefs.ts");
const { copy } = await import("../src/copy.ts");
const { AudioPrefsStore, MockAudioPrefs } = audioPrefs;

const validPrefs = {
  master_volume_percent: 60,
  hotkeys_enabled: true,
  hotkeys_registered: true,
  muted: false,
};
const validEvent = { percent: 35, source: "ui" };

function client(overrides: Record<string, unknown> = {}) {
  let onVolume: ((event: unknown) => void) | null = null;
  return {
    get: async () => validPrefs,
    setVolume: async (percent: number) => ({ ...validPrefs, master_volume_percent: percent }),
    setHotkeys: async (enabled: boolean) => ({ ...validPrefs, hotkeys_enabled: enabled }),
    subscribeVolume: async (callback: (event: unknown) => void) => {
      onVolume = callback;
      return () => { onVolume = null; };
    },
    sendVolume(event: unknown) { onVolume?.(event); },
    ...overrides,
  };
}

async function flushPromises() {
  for (let index = 0; index < 8; index += 1) await Promise.resolve();
}

test("audio prefs parser accepts the contract and nullable muted state", () => {
  assert.deepEqual(audioPrefs.parseAudioPrefs(validPrefs), validPrefs);
  assert.deepEqual(audioPrefs.parseAudioPrefs({ ...validPrefs, muted: null }), { ...validPrefs, muted: null });
  assert.deepEqual(audioPrefs.parseAudioPrefs({ ...validPrefs, hotkeys_registered: null }), { ...validPrefs, hotkeys_registered: null });
});

test("audio prefs parser rejects non-object, missing, and extra-field replies", () => {
  assert.equal(audioPrefs.parseAudioPrefs(null), null);
  assert.equal(audioPrefs.parseAudioPrefs([]), null);
  assert.equal(audioPrefs.parseAudioPrefs({ ...validPrefs, muted: undefined }), null);
  assert.equal(audioPrefs.parseAudioPrefs({ ...validPrefs, extra: true }), null);
});

test("audio prefs parser rejects a string volume", () => {
  assert.equal(audioPrefs.parseAudioPrefs({ ...validPrefs, master_volume_percent: "60" }), null);
});

test("audio prefs parser rejects a fractional volume", () => {
  assert.equal(audioPrefs.parseAudioPrefs({ ...validPrefs, master_volume_percent: 60.5 }), null);
});

test("audio prefs parser rejects a volume below zero", () => {
  assert.equal(audioPrefs.parseAudioPrefs({ ...validPrefs, master_volume_percent: -1 }), null);
});

test("audio prefs parser rejects a volume above one hundred", () => {
  assert.equal(audioPrefs.parseAudioPrefs({ ...validPrefs, master_volume_percent: 101 }), null);
});

test("audio prefs parser rejects a non-boolean hotkeys_enabled field", () => {
  assert.equal(audioPrefs.parseAudioPrefs({ ...validPrefs, hotkeys_enabled: 1 }), null);
});

test("audio prefs parser rejects a non-boolean hotkeys_registered field", () => {
  assert.equal(audioPrefs.parseAudioPrefs({ ...validPrefs, hotkeys_registered: "yes" }), null);
});

test("audio prefs parser rejects a non-boolean muted field", () => {
  assert.equal(audioPrefs.parseAudioPrefs({ ...validPrefs, muted: 0 }), null);
});

test("volume event parser accepts UI and hotkey events", () => {
  assert.deepEqual(audioPrefs.parseAudioVolumeEvent(validEvent), validEvent);
  assert.deepEqual(audioPrefs.parseAudioVolumeEvent({ percent: 100, source: "hotkey" }), { percent: 100, source: "hotkey" });
});

test("volume event parser rejects wrong shape and wrong field types", () => {
  assert.equal(audioPrefs.parseAudioVolumeEvent(null), null);
  assert.equal(audioPrefs.parseAudioVolumeEvent({ ...validEvent, extra: true }), null);
  assert.equal(audioPrefs.parseAudioVolumeEvent({ percent: "35", source: "ui" }), null);
});

test("volume event parser rejects fractional and out-of-range percentages", () => {
  for (const percent of [-1, 101, 25.5]) {
    assert.equal(audioPrefs.parseAudioVolumeEvent({ percent, source: "ui" }), null);
  }
});

test("volume event parser rejects unknown event sources", () => {
  assert.equal(audioPrefs.parseAudioVolumeEvent({ percent: 35, source: "system" }), null);
});

test("typed audio client calls the three host commands with their contract arguments", async () => {
  const calls: Array<[string, Record<string, unknown> | undefined]> = [];
  const typed = audioPrefs.createAudioPrefsClient({
    invoke: async (command: string, args?: Record<string, unknown>) => {
      calls.push([command, args]);
      if (command === "audio_set_volume") return { ...validPrefs, master_volume_percent: args?.percent };
      if (command === "audio_set_hotkeys") return { ...validPrefs, hotkeys_enabled: args?.enabled };
      return validPrefs;
    },
    listen: async () => () => {},
  });

  assert.deepEqual(await typed.get(), validPrefs);
  assert.deepEqual(await typed.setVolume(25), { ...validPrefs, master_volume_percent: 25 });
  assert.deepEqual(await typed.setHotkeys(false), { ...validPrefs, hotkeys_enabled: false });
  assert.deepEqual(calls, [
    ["audio_prefs_get", undefined],
    ["audio_set_volume", { percent: 25 }],
    ["audio_set_hotkeys", { enabled: false }],
  ]);
});

test("typed audio client turns malformed replies and invoke failures into unknown", async () => {
  const typed = audioPrefs.createAudioPrefsClient({
    invoke: async (command: string) => command === "audio_prefs_get" ? { ...validPrefs, extra: true } : Promise.reject(new Error("offline")),
    listen: async () => () => {},
  });
  assert.equal(await typed.get(), null);
  assert.equal(await typed.setVolume(10), null);
  assert.equal(await typed.setHotkeys(true), null);
});

test("typed audio client filters malformed volume events from the host listener", async () => {
  let listener: ((payload: unknown) => void) | null = null;
  let eventName = "";
  const typed = audioPrefs.createAudioPrefsClient({
    invoke: async () => validPrefs,
    listen: async (event: string, callback: (payload: unknown) => void) => {
      eventName = event;
      listener = callback;
      return () => { listener = null; };
    },
  });
  const seen: unknown[] = [];
  await typed.subscribeVolume((event: unknown) => seen.push(event));
  listener?.({ percent: 101, source: "hotkey" });
  listener?.({ percent: 40, source: "hotkey" });
  assert.equal(eventName, "audio://volume");
  assert.deepEqual(seen, [{ percent: 40, source: "hotkey" }]);
});

test("typed audio client rejects invalid command arguments without invoking the host", async () => {
  let calls = 0;
  const typed = audioPrefs.createAudioPrefsClient({
    invoke: async () => { calls += 1; return validPrefs; },
    listen: async () => () => {},
  });
  assert.equal(await typed.setVolume(100.5), null);
  assert.equal(await typed.setVolume(-1), null);
  assert.equal(await typed.setHotkeys("yes" as never), null);
  assert.equal(calls, 0);
});

test("trailing throttle coalesces a same-turn burst to its last value", (t) => {
  t.mock.timers.enable({ apis: ["setTimeout", "Date"] });
  const sent: number[] = [];
  const throttle = audioPrefs.createTrailingThrottle((value: number) => sent.push(value), 120);
  throttle.push(10);
  throttle.push(30);
  throttle.push(45);
  assert.deepEqual(sent, []);
  t.mock.timers.tick(0);
  assert.deepEqual(sent, [45]);
  throttle.dispose();
});

test("trailing throttle keeps 120ms spacing and sends the most recent value", (t) => {
  t.mock.timers.enable({ apis: ["setTimeout", "Date"] });
  const sent: number[] = [];
  const throttle = audioPrefs.createTrailingThrottle((value: number) => sent.push(value), 120);
  throttle.push(10);
  t.mock.timers.tick(0);
  throttle.push(20);
  t.mock.timers.tick(50);
  throttle.push(30);
  t.mock.timers.tick(69);
  assert.deepEqual(sent, [10]);
  t.mock.timers.tick(1);
  assert.deepEqual(sent, [10, 30]);
  throttle.dispose();
});

test("trailing throttle sends a new value immediately after its spacing window", (t) => {
  t.mock.timers.enable({ apis: ["setTimeout", "Date"] });
  const sent: number[] = [];
  const throttle = audioPrefs.createTrailingThrottle((value: number) => sent.push(value), 120);
  throttle.push(10);
  t.mock.timers.tick(0);
  t.mock.timers.tick(120);
  throttle.push(25);
  t.mock.timers.tick(0);
  assert.deepEqual(sent, [10, 25]);
  throttle.dispose();
});

test("unknown volume model shows unknown copy and no guessed percentage", () => {
  const text = copy("th").audio.volume;
  const model = audioPrefs.volumeControlModel(null, null, text);
  assert.equal(model.unknown, true);
  assert.equal(model.percent, null);
  assert.equal(model.readout, "ไม่ทราบระดับเสียง");
});

test("volume model keeps the shared draft value visible over the confirmed host value", () => {
  const model = audioPrefs.volumeControlModel(validPrefs, 25, copy("en").audio.volume);
  assert.equal(model.percent, 25);
  assert.equal(model.readout, "25%");
  assert.equal(model.unknown, false);
});

test("muted volume model uses the localized applies-when-unmuted message", () => {
  const model = audioPrefs.volumeControlModel({ ...validPrefs, muted: true }, null, copy("th").audio.volume);
  assert.equal(model.mutedMessage, "ปิดเสียงอยู่ — ระดับเสียงมีผลเมื่อเปิดเสียง");
  const english = audioPrefs.volumeControlModel({ ...validPrefs, muted: true }, null, copy("en").audio.volume);
  assert.equal(english.mutedMessage, "Muted — the level takes effect when audio is on");
});

test("hotkeys model exposes enabled, disabled, and registration-conflict states", () => {
  const text = copy("th").audio.volume;
  const enabled = audioPrefs.volumeControlModel(validPrefs, null, text);
  const disabled = audioPrefs.volumeControlModel({ ...validPrefs, hotkeys_enabled: false }, null, text);
  const conflict = audioPrefs.volumeControlModel({ ...validPrefs, hotkeys_registered: false }, null, text);
  assert.equal(enabled.hotkeysEnabled, true);
  assert.equal(disabled.hotkeysEnabled, false);
  assert.equal(conflict.hotkeysRegistrationFailed, true);
  assert.equal(conflict.hotkeysRegistrationMessage, "ลงทะเบียนปุ่มลัดไม่ได้ อาจชนกับโปรแกรมอื่น");
});

test("toast controller has no visible value before a volume event", () => {
  const visible: unknown[] = [];
  const toast = new audioPrefs.TransientNotice((value: unknown) => visible.push(value), 1_500);
  assert.deepEqual(visible, []);
  toast.dispose();
});

test("toast appears at once and hides after 1.5 seconds", (t) => {
  t.mock.timers.enable({ apis: ["setTimeout"] });
  const visible: unknown[] = [];
  const toast = new audioPrefs.TransientNotice((value: unknown) => visible.push(value), 1_500);
  toast.show(validEvent);
  assert.deepEqual(visible, [validEvent]);
  t.mock.timers.tick(1_499);
  assert.deepEqual(visible, [validEvent]);
  t.mock.timers.tick(1);
  assert.deepEqual(visible, [validEvent, null]);
  toast.dispose();
});

test("a new toast event resets the 1.5-second hide timer", (t) => {
  t.mock.timers.enable({ apis: ["setTimeout"] });
  const visible: unknown[] = [];
  const toast = new audioPrefs.TransientNotice((value: unknown) => visible.push(value), 1_500);
  toast.show(validEvent);
  t.mock.timers.tick(1_000);
  const nextEvent = { percent: 70, source: "hotkey" };
  toast.show(nextEvent);
  t.mock.timers.tick(1_499);
  assert.deepEqual(visible, [validEvent, nextEvent]);
  t.mock.timers.tick(1);
  assert.deepEqual(visible, [validEvent, nextEvent, null]);
  toast.dispose();
});

test("shared prefs store leaves prefs and volume event unknown after a malformed initial reply", async () => {
  const store = new AudioPrefsStore(client({ get: async () => ({ ...validPrefs, muted: "unknown" }) }));
  store.start();
  await flushPromises();
  assert.equal(store.getSnapshot().loading, false);
  assert.equal(store.getSnapshot().prefs, null);
  assert.equal(store.getSnapshot().volumeEvent, null);
});

test("failed volume change retains the last confirmed host value and reports an error", async (t) => {
  t.mock.timers.enable({ apis: ["setTimeout"] });
  const store = new AudioPrefsStore(client({ setVolume: async () => null }));
  store.start();
  await flushPromises();
  store.setVolume(20);
  t.mock.timers.tick(0);
  await flushPromises();
  assert.equal(store.getSnapshot().prefs?.master_volume_percent, 60);
  assert.equal(store.getSnapshot().draftPercent, null);
  assert.equal(store.getSnapshot().error, "volume_failed");
});

test("initial prefs loading does not synthesize a volume toast", async () => {
  const store = new AudioPrefsStore(client());
  store.start();
  await flushPromises();
  assert.equal(store.getSnapshot().volumeEvent, null);
});

test("mock audio prefs round trip starts at 60 percent with hotkeys on", async () => {
  const mock = new MockAudioPrefs();
  assert.deepEqual(await mock.get(), {
    master_volume_percent: 60,
    hotkeys_enabled: true,
    hotkeys_registered: true,
    muted: false,
  });
  const events: unknown[] = [];
  mock.subscribeVolume((event: unknown) => events.push(event));
  assert.equal((await mock.setVolume(35)).master_volume_percent, 35);
  assert.deepEqual(events, [{ percent: 35, source: "ui" }]);
  assert.equal((await mock.setHotkeys(false)).hotkeys_enabled, false);
});

test("mock emits hotkey volume events only when the registered shortcut is enabled", async () => {
  const mock = new MockAudioPrefs();
  const events: unknown[] = [];
  mock.subscribeVolume((event: unknown) => events.push(event));
  mock.setRegistrationResult(false);
  assert.equal(mock.changeFromHotkey(45), false);
  await mock.setHotkeys(false);
  assert.equal(mock.changeFromHotkey(45), false);
  mock.setRegistrationResult(true);
  await mock.setHotkeys(true);
  assert.equal(mock.changeFromHotkey(45), true);
  assert.deepEqual(events, [{ percent: 45, source: "hotkey" }]);
});

test("mock harness wires all volume commands and the host event name", () => {
  const harness = readFileSync(new URL("../src/mock/harness.tsx", import.meta.url), "utf8");
  for (const command of ["audio_prefs_get", "audio_set_volume", "audio_set_hotkeys"]) {
    assert.match(harness, new RegExp(command));
  }
  assert.match(harness, /audio:\/\/volume/);
});

test("volume labels, error messages, and Dota hotkey hint exist in both locales", () => {
  const thai = copy("th").audio.volume;
  const english = copy("en").audio.volume;
  assert.equal(thai.announcerVolume, "ระดับเสียงประกาศ");
  assert.equal(thai.mutedMessage, "ปิดเสียงอยู่ — ระดับเสียงมีผลเมื่อเปิดเสียง");
  assert.equal(thai.hotkeysLabel, "ปุ่มลัด Alt+↑ / Alt+↓ ปรับเสียงทั้งระบบ");
  assert.ok(thai.hotkeysHint.includes("Dota 2"));
  assert.ok(english.announcerVolume.includes("Announcer"));
  assert.ok(english.hotkeysHint.includes("Dota 2"));
  assert.ok(english.changeFailed.length > 0 && thai.changeFailed.length > 0);
});

test("deck and pack placements use the same VolumeControl and shared prefs hook", () => {
  const deck = readFileSync(new URL("../src/deck/AnnouncerStatus.tsx", import.meta.url), "utf8");
  const packs = readFileSync(new URL("../src/PackPanel.tsx", import.meta.url), "utf8");
  const control = readFileSync(new URL("../src/VolumeControl.tsx", import.meta.url), "utf8");
  const hook = readFileSync(new URL("../src/useAudioPrefs.ts", import.meta.url), "utf8");
  assert.match(deck, /<VolumeControl/);
  assert.match(packs, /<VolumeControl/);
  assert.match(control, /useAudioPrefs\(/);
  assert.match(hook, /new AudioPrefsStore\(/);
});

test("volume toast is mounted in the app shell and announces status politely", () => {
  const app = readFileSync(new URL("../src/App.tsx", import.meta.url), "utf8");
  const toast = readFileSync(new URL("../src/VolumeToast.tsx", import.meta.url), "utf8");
  assert.match(app, /<VolumeToast/);
  assert.match(toast, /role="status"/);
  assert.match(toast, /aria-live="polite"/);
});
