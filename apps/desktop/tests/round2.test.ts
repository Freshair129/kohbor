import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { test } from "node:test";
import { GuestAccountPort } from "../src/account/accountView.ts";
import { reduceAccountMenu } from "../src/account/menuModel.ts";
import { parseSteamStatus, deriveLaunchView, launchResultText, steamPersonaName, deriveSteamChips } from "../src/steam/model.ts";
import { connectSteamStatus, createDotaLauncher } from "../src/steam/controller.ts";
import { readShowSteamName, writeShowSteamName } from "../src/steam/steamName.ts";
import { deriveMinimapState, createRevealState, reduceReveal, revealDuration, minimapPlaceholderText, type MinimapState } from "../src/live/minimapStageModel.ts";
import { shellTabs } from "../src/shell/state.ts";

const steam = (changes = {}) => parseSteamStatus({ steam_running: true, dota_running: false, persona_name: "local-persona",
  gsi: "waiting", game_phase: "none", launch_pending: false, minimap_state: "not_running", ...changes })!;
const capture = (phase: string, reason: string | null = null) => ({ phase, phase_reason: reason });

test("guest account port has no identity, permissions or state changes from reserved methods", async () => {
  const port = new GuestAccountPort(); let called = 0;
  const unsubscribe = port.subscribe(() => { called++; });
  assert.deepEqual(port.get(), { state: "guest", displayName: null, avatar: null, canSignIn: false, canSignOut: false, friendsAvailable: false });
  await port.signIn(); await port.signOut(); unsubscribe();
  assert.equal(port.get().state, "guest"); assert.equal(called, 0);
});

test("account menu opens, wraps enabled entries, closes on Escape and returns focus", () => {
  let state = { open: false, index: 0 };
  const down = reduceAccountMenu(state, "ArrowDown", 3); state = down.state;
  assert.deepEqual(state, { open: true, index: 0 });
  state = reduceAccountMenu(state, "ArrowUp", 3).state; assert.equal(state.index, 2);
  state = reduceAccountMenu(state, "ArrowDown", 3).state; assert.equal(state.index, 0);
  state = reduceAccountMenu(state, "End", 3).state; assert.equal(state.index, 2);
  state = reduceAccountMenu(state, "Home", 3).state; assert.equal(state.index, 0);
  const close = reduceAccountMenu(state, "Escape", 3);
  assert.equal(close.state.open, false); assert.equal(close.returnFocus, true);
  assert.equal(reduceAccountMenu(state, "Tab", 3).returnFocus, false);
  assert.equal(reduceAccountMenu(state, "outside", 3).state.open, false);
  assert.equal(reduceAccountMenu(state, "select", 3).returnFocus, true);
  assert.equal(reduceAccountMenu({ open: false, index: 0 }, "ArrowUp", 3).state.index, 2);
});

test("Steam parser keeps nullable process/persona fields unknown and rejects incomplete contracts", () => {
  const unknown = steam({ steam_running: null, dota_running: null, persona_name: null, game_phase: "unknown", gsi: "unknown" });
  assert.equal(unknown.dota_running, null);
  for (const bad of [null, {}, { ...steam(), launch_pending: null }, { ...steam(), minimap_state: "invented" }]) assert.equal(parseSteamStatus(bad), null);
  assert.deepEqual(deriveSteamChips(null, "th"), { steam: "—", dota: "—", gsi: "—" });
  assert.deepEqual(deriveSteamChips(steam(), "th"), { steam: "ออนไลน์", dota: "ปิดอยู่", gsi: "รอ" });
  assert.deepEqual(deriveSteamChips(steam({ steam_running: false, dota_running: true, gsi: "receiving" }), "th"), { steam: "ไม่พบ", dota: "กำลังทำงาน", gsi: "รับข้อมูล" });
});

test("Steam persona is opt-in display text separate from the guest account", () => {
  assert.equal(readShowSteamName(null), true);
  assert.equal(readShowSteamName({ getItem: () => "false" }), false);
  assert.equal(readShowSteamName({ getItem: () => { throw new Error(); } }), true);
  let saved: unknown; writeShowSteamName({ setItem: (...args) => { saved = args; } }, false);
  assert.deepEqual(saved, ["kohbor.showSteamName", "false"]);
  assert.doesNotThrow(() => writeShowSteamName({ setItem: () => { throw new Error(); } }, true));
  assert.equal(steamPersonaName(steam(), false), null); assert.equal(steamPersonaName(steam(), true), "local-persona");
  assert.equal(steamPersonaName(null, true), null); assert.equal(new GuestAccountPort().get().displayName, null);
});

test("Start Dota requires explicit false, no pending launch and no pregame/live match", () => {
  assert.equal(deriveLaunchView(steam(), false, "th").enabled, true);
  assert.equal(deriveLaunchView(steam({ steam_running: false, game_phase: "unknown" }), false, "th").enabled, true);
  const cases = [
    [null, false, "ตรวจสถานะไม่ได้"], [steam({ dota_running: null }), false, "ตรวจสถานะไม่ได้"],
    [steam({ dota_running: true }), false, "เกมกำลังทำงาน"], [steam({ launch_pending: true }), false, "กำลังเปิดอยู่"],
    [steam(), true, "กำลังเปิดอยู่"], [steam({ game_phase: "pregame" }), false, "กำลังอยู่ในแมตช์"],
    [steam({ game_phase: "live", dota_running: true }), false, "กำลังอยู่ในแมตช์"],
  ] as const;
  for (const [status, busy, reason] of cases) assert.deepEqual(deriveLaunchView(status, busy, "th"), { enabled: false, reason });
});

test("every typed launch result is localized and a missing result stays unknown", () => {
  const messages = { launched: "ส่งคำสั่งเปิดผ่าน Steam แล้ว", steam_not_found: "ไม่พบ Steam", already_running: "เกมกำลังทำงาน",
    blocked_live_match: "กำลังอยู่ในแมตช์", cooldown: "เพิ่งส่งคำสั่ง — รอสักครู่", failed: "เปิด Dota 2 ไม่สำเร็จ" };
  for (const [result, text] of Object.entries(messages)) assert.equal(launchResultText({ result, detail: null } as never, "th"), text);
  assert.equal(launchResultText({ result: "failed", detail: "host detail" }, "th"), "เปิด Dota 2 ไม่สำเร็จ · host detail");
  assert.equal(launchResultText(null, "th"), "ตรวจผลการเปิดไม่ได้");
});

test("launch controller guards double clicks and never retries or auto-launches", async () => {
  let calls = 0; let resolve!: (value: unknown) => void;
  const launcher = createDotaLauncher(async command => { assert.equal(command, "launch_dota"); calls++; return new Promise(done => { resolve = done; }); });
  assert.equal(calls, 0); await launcher.launch(null); assert.equal(calls, 0);
  const pending = launcher.launch(steam()); await launcher.launch(steam()); assert.equal(calls, 1);
  resolve({ result: "launched", detail: null }); await pending;
  assert.equal(launcher.getSnapshot().result?.result, "launched"); assert.equal(calls, 1);
  const duplicate = launcher.launch(steam());
  resolve({ result: "cooldown", detail: null }); await duplicate;
  assert.equal(calls, 1, "fast successful IPC must not reopen the double-click window");
  assert.equal(launcher.getSnapshot().awaitingStatus, true);
  launcher.observeStatus(); assert.equal(launcher.getSnapshot().awaitingStatus, false); assert.equal(calls, 1);
  const missing = createDotaLauncher(async () => { throw new Error("missing command"); });
  await missing.launch(steam()); assert.equal(missing.getSnapshot().result, null); assert.equal(missing.getSnapshot().pending, false);
});

test("Steam subscribes before initial read, newer events beat a late reply and cleanup unsubscribes", async () => {
  const order: string[] = []; const seen: unknown[] = []; let emit!: (value: unknown) => void;
  let resolve!: (value: unknown) => void; let stopped = false;
  const stop = connectSteamStatus({ subscribe: async cb => { order.push("listen"); emit = cb; return () => { stopped = true; }; },
    get: async () => { order.push("get"); return new Promise(done => { resolve = done; }); } }, value => seen.push(value));
  await Promise.resolve(); assert.deepEqual(order, ["listen", "get"]);
  emit(steam({ dota_running: true })); resolve(steam()); await new Promise(done => setImmediate(done));
  assert.equal((seen.at(-1) as { dota_running: boolean }).dota_running, true);
  stop(); assert.equal(stopped, true); emit(steam()); assert.equal(seen.length, 1);
});

test("older hosts without Steam commands resolve to unknown", async () => {
  const seen: unknown[] = [];
  const stop = connectSteamStatus({ subscribe: async () => { throw new Error(); }, get: async () => { throw new Error(); } }, value => seen.push(value));
  await new Promise(done => setImmediate(done)); assert.deepEqual(seen, [null]); stop();
});

const states: MinimapState[] = ["not_running", "launching", "hero_selection", "loading", "first_frame", "live", "occluded", "unsupported"];
test("host minimap values own the state; capture-only fallback never invents a process state", () => {
  for (const minimap_state of states) assert.equal(deriveMinimapState(steam({ minimap_state }), capture("live")), minimap_state);
  for (const [phase, reason, expected] of [["idle", "waiting_for_game", "loading"], ["calibrating", null, "first_frame"],
    ["live", null, "live"], ["stale", "occluded", "occluded"], ["stale", null, "unsupported"],
    ["lost", null, "unsupported"], ["unsupported", null, "unsupported"]] as const) {
    assert.equal(deriveMinimapState(null, capture(phase, reason)), expected);
  }
  assert.equal(deriveMinimapState(null, null), "unsupported");
});

test("every placeholder has honest Thai copy and waiting states are calm", () => {
  const messages = { not_running: "Dota 2 ยังไม่เปิด — กดเริ่ม Dota 2", launching: "กำลังเปิด Dota 2…",
    hero_selection: "กำลังเลือกฮีโร่ — แผนที่จะปรากฏเมื่อเกมเริ่ม", loading: "กำลังโหลดเกม — รอแผนที่" };
  for (const [state, text] of Object.entries(messages)) assert.equal(minimapPlaceholderText(state as MinimapState, "th"), text);
});

test("local capture stops retain their reason plate and idle fallback does not invent a loading game", () => {
  assert.equal(deriveMinimapState(steam({ minimap_state: "live" }), capture("live"), true), "unsupported");
  assert.equal(deriveMinimapState(steam({ minimap_state: "occluded" }), capture("stale", "occluded"), true), "unsupported");
  const waiting = "รอเกมเริ่ม — แผนที่ย่อจะแสดงเมื่อเข้าช่วงก่อนเริ่มเกม";
  assert.equal(minimapPlaceholderText("loading", "th", waiting), waiting);
});

test("first_frame reveals only after an admitted painted frame, with finite normal/reduced durations", () => {
  for (const reducedMotion of [false, true]) {
    let state = reduceReveal(createRevealState(), { type: "sync", source: "first_frame", hasFrame: false, reducedMotion });
    assert.equal(state.view, "loading"); assert.equal(state.revealId, 0);
    state = reduceReveal(state, { type: "sync", source: "first_frame", hasFrame: true, reducedMotion });
    assert.equal(state.view, "first_frame"); assert.equal(revealDuration(state), reducedMotion ? 160 : 760);
    const again = reduceReveal(state, { type: "sync", source: "first_frame", hasFrame: true, reducedMotion });
    assert.equal(again.revealId, state.revealId);
    state = reduceReveal(again, { type: "finish", revealId: state.revealId }); assert.equal(state.view, "live");
    assert.equal(reduceReveal(state, { type: "sync", source: "first_frame", hasFrame: true, reducedMotion }).view, "live");
  }
});

test("host live arriving before paint preserves acquisition; rejected first frames cannot reveal", () => {
  let state = reduceReveal(createRevealState(), { type: "sync", source: "first_frame", hasFrame: false, reducedMotion: false });
  state = reduceReveal(state, { type: "sync", source: "live", hasFrame: false, reducedMotion: false });
  assert.equal(state.view, "loading"); assert.equal(state.revealId, 0);
  state = reduceReveal(state, { type: "sync", source: "live", hasFrame: true, reducedMotion: false });
  assert.equal(state.view, "first_frame");
});

test("all state transitions cancel reveal appropriately and stale completion cannot reopen a cover", () => {
  for (const before of states) for (const source of states) {
    const old = reduceReveal(createRevealState(), { type: "sync", source: before, hasFrame: true, reducedMotion: false });
    const next = reduceReveal(old, { type: "sync", source, hasFrame: true, reducedMotion: false });
    const expected = source === "live" && before === "first_frame" ? "first_frame" : source;
    assert.equal(next.view, expected, `${before} -> ${source}`);
    if (!["live", "first_frame"].includes(source)) {
      assert.equal(next.view, source, `${before} -> ${source}`);
      assert.equal(reduceReveal(next, { type: "finish", revealId: old.revealId }).view, source);
    }
  }
  const noFrame = reduceReveal(createRevealState(), { type: "sync", source: "live", hasFrame: false, reducedMotion: true });
  assert.equal(noFrame.view, "loading");
  const first = reduceReveal(createRevealState(), { type: "sync", source: "first_frame", hasFrame: true, reducedMotion: false });
  const canceled = reduceReveal(first, { type: "sync", source: "loading", hasFrame: false, reducedMotion: false });
  const second = reduceReveal(canceled, { type: "sync", source: "first_frame", hasFrame: true, reducedMotion: false });
  assert.equal(reduceReveal(second, { type: "finish", revealId: first.revealId }).view, "first_frame");
});

test("tab bar contains Live/Packs/Status and optional Logs; account/settings/friends stay outside", () => {
  assert.deepEqual(shellTabs(false), ["live", "packs", "status"]);
  assert.deepEqual(shellTabs(true), ["live", "packs", "status", "logs"]);
});

test("new shell, reserved pages and stage remain bounded; reveal adds duration tokens only", () => {
  const css = ["../src/account/account.css", "../src/live/minimapStage.css", "../src/steam/steam.css"].map(path => readFileSync(new URL(path, import.meta.url), "utf8")).join("\n");
  assert.doesNotMatch(css, /#[0-9a-f]{3,8}\b|rgba?\(/i);
  assert.doesNotMatch(css, /overflow(?:-y)?:\s*(auto|scroll)/);
  assert.match(css, /--kb-reveal-duration:\s*760ms/); assert.match(css, /--kb-reduced-duration:\s*160ms/);
  assert.match(css, /prefers-reduced-motion:\s*reduce/); assert.doesNotMatch(css, /infinite/);
  for (const selector of ["account-page", "friends-page", "minimap-stage"]) {
    assert.match(css, new RegExp(`\\.${selector}\\s*\\{[^}]*min-height:\\s*0[^}]*overflow:\\s*hidden`));
  }
  const app = readFileSync(new URL("../src/App.tsx", import.meta.url), "utf8");
  assert.match(app, /AccountMenu/); assert.match(app, /StartDotaBar/);
  const shell = readFileSync(new URL("../src/styles.css", import.meta.url), "utf8");
  for (const selector of ["shell-header", "shell-mainbar", "shell-statusbar"]) {
    assert.match(shell, new RegExp(`\\.${selector}\\s*\\{[^}]*min-height:\\s*0[^}]*overflow:\\s*hidden`));
  }
  assert.match(shell, /@media\s*\(max-height:\s*700px\)\s*\{[\s\S]*?\.shell-header\s*\{[^}]*height:\s*84px[^}]*grid-template-rows:\s*46px minmax\(0,\s*1fr\)/);
  const pipeline = readFileSync(new URL("../src/live/MinimapPanel.tsx", import.meta.url), "utf8");
  assert.match(pipeline, /<MinimapStage/); assert.match(pipeline, /ref=\{minimap\.canvasRef\}/);
  const stage = readFileSync(new URL("../src/live/MinimapStage.tsx", import.meta.url), "utf8");
  assert.doesNotMatch(stage, /setInterval|setTimeout|\bemit\(/);
  const pages = readFileSync(new URL("../src/account/AccountPages.tsx", import.meta.url), "utf8");
  assert.match(pages, /เร็วๆ นี้/); assert.match(pages, /ยังไม่มีข้อมูลจำนวนเพื่อน/);
  assert.doesNotMatch(pages, /KB-MOCK|KB-DEMO|SteamID|steam_persona/);
});
