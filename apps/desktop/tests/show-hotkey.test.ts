import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { test } from "node:test";

const model = await import("../src/showHotkey.ts");
const { parseShowHotkey, captureKey, describeError, SHOW_HOTKEY_COPY } = model;

const valid = {
  enabled: true,
  binding: "Ctrl+Alt+X",
  registered: true,
  error_code: null,
  dota_conflict_risk: false,
};

function press(code: string, mods: Partial<{ ctrlKey: boolean; altKey: boolean; shiftKey: boolean; metaKey: boolean }> = {}) {
  return { ctrlKey: false, altKey: false, shiftKey: false, metaKey: false, code, ...mods };
}

test("show hotkey parser accepts the host contract", () => {
  assert.deepEqual(parseShowHotkey(valid), valid);
  assert.deepEqual(parseShowHotkey({ ...valid, error_code: "SHOW_HOTKEY_REGISTER_FAILED:1409", registered: false }),
    { ...valid, error_code: "SHOW_HOTKEY_REGISTER_FAILED:1409", registered: false });
});

test("show hotkey parser keeps a malformed reply unknown", () => {
  assert.equal(parseShowHotkey(null), null);
  assert.equal(parseShowHotkey([]), null);
  assert.equal(parseShowHotkey({ ...valid, extra: 1 }), null);
  const { registered: _drop, ...missing } = valid;
  assert.equal(parseShowHotkey(missing), null);
  assert.equal(parseShowHotkey({ ...valid, enabled: "yes" }), null);
  assert.equal(parseShowHotkey({ ...valid, binding: "" }), null);
  assert.equal(parseShowHotkey({ ...valid, error_code: 5 }), null);
});

test("capture builds canonical labels", () => {
  assert.deepEqual(captureKey(press("KeyX", { ctrlKey: true, altKey: true })), { kind: "binding", label: "Ctrl+Alt+X" });
  assert.deepEqual(captureKey(press("Digit7", { altKey: true, metaKey: true })), { kind: "binding", label: "Alt+Win+7" });
  assert.deepEqual(captureKey(press("F9", { ctrlKey: true, shiftKey: true })), { kind: "binding", label: "Ctrl+Shift+F9" });
});

test("capture waits on modifiers, cancels on Escape, refuses other keys", () => {
  assert.deepEqual(captureKey(press("ControlLeft", { ctrlKey: true })), { kind: "pending" });
  assert.deepEqual(captureKey(press("AltRight", { altKey: true })), { kind: "pending" });
  assert.deepEqual(captureKey(press("Escape")), { kind: "cancel" });
  assert.deepEqual(captureKey(press("KeyX")), { kind: "invalid" });
  assert.deepEqual(captureKey(press("KeyX", { shiftKey: true })), { kind: "invalid" });
  assert.deepEqual(captureKey(press("Tab", { ctrlKey: true })), { kind: "invalid" });
  assert.deepEqual(captureKey(press("F13", { ctrlKey: true })), { kind: "invalid" });
});

test("errors are explained and the raw code stays visible", () => {
  assert.match(describeError("SHOW_HOTKEY_REGISTER_FAILED:1409", "th"), /\(SHOW_HOTKEY_REGISTER_FAILED:1409\)$/);
  assert.equal(describeError("SOMETHING_NEW", "en"), "SOMETHING_NEW");
});

test("copy discloses the fullscreen limit and the no-input promise in both locales", () => {
  for (const locale of ["th", "en"] as const) {
    const copy = SHOW_HOTKEY_COPY[locale];
    assert.match(copy.fullscreen, /borderless/);
    assert.match(copy.fullscreen, /exclusive fullscreen/);
    assert.ok(copy.help.length > 0 && copy.dotaRisk.includes("Dota"));
  }
});

test("settings page mounts the show hotkey section and runtime calls the host commands", () => {
  const app = readFileSync(new URL("../src/App.tsx", import.meta.url), "utf8");
  assert.match(app, /<ShowHotkeySettings locale=\{displayLocale\} \/>/);
  const runtime = readFileSync(new URL("../src/runtime.ts", import.meta.url), "utf8");
  assert.match(runtime, /invoke<unknown>\("shell_hotkey_get"\)/);
  assert.match(runtime, /invoke<unknown>\("shell_hotkey_set", \{ enabled, binding \}\)/);
});
