import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { registerHooks } from "node:module";
import { test } from "node:test";
import type { UpdateReply, UpdateState } from "../src/shell/updateModel.ts";

registerHooks({
  resolve(specifier, context, nextResolve) {
    if ((specifier.startsWith("./") || specifier.startsWith("../")) && !/\.[a-z]+$/i.test(specifier)) {
      try {
        return nextResolve(specifier, context);
      } catch {
        return nextResolve(`${specifier}.ts`, context);
      }
    }
    return nextResolve(specifier, context);
  },
});

const { createUpdateConfirmFlow, parseUpdateReply, updateView } = await import("../src/shell/updateModel.ts");
const { updateCopy } = await import("../src/shell/updateCopy.ts");
const { createUpdateStatusStore } = await import("../src/shell/updateStatusStore.ts");
const { createMockUpdater, parseMockUpdateScenario } = await import("../src/mock/updateHarness.ts");

const available: UpdateReply = {
  state: "available",
  current_version: "0.3.0-dev.15",
  reason: null,
  available: { version: "0.3.1", notes: "First line\nSecond line", pub_date: null },
  download_progress: null,
  match_active: false,
};

function reply(state: UpdateState, reason: string | null = null, match_active = false): UpdateReply {
  const hasCandidate = ["available", "downloading", "ready", "installing"].includes(state);
  return {
    state,
    current_version: "0.3.0-dev.15",
    reason,
    available: hasCandidate
      ? { version: "0.3.1", notes: "First line\nSecond line", pub_date: null }
      : null,
    download_progress: state === "downloading" ? 42 : null,
    match_active,
  };
}

test("strict parser accepts complete reply shapes and rejects malformed payloads", () => {
  assert.deepEqual(parseUpdateReply(available), available);
  assert.deepEqual(parseUpdateReply(reply("not_configured", "BUILD_WITHOUT_UPDATER")), reply("not_configured", "BUILD_WITHOUT_UPDATER"));
  for (const value of [null, [], "reply", 4, true]) assert.equal(parseUpdateReply(value), null);
  assert.equal(parseUpdateReply({ ...available, state: "installed" }), null);
  assert.equal(parseUpdateReply({ ...available, current_version: 4 }), null);
  assert.equal(parseUpdateReply({ ...available, reason: false }), null);
  assert.equal(parseUpdateReply({ ...available, available: { ...available.available!, notes: null } }), null);
  assert.equal(parseUpdateReply({ ...available, available: { ...available.available!, pub_date: 5 } }), null);
  assert.equal(parseUpdateReply({ ...available, download_progress: 101 }), null);
  assert.equal(parseUpdateReply({ ...available, download_progress: 2.5 }), null);
  assert.equal(parseUpdateReply({ ...available, match_active: "false" }), null);
  const { download_progress: _progress, ...missingProgress } = available;
  assert.equal(parseUpdateReply(missingProgress), null);
  assert.equal(parseUpdateReply({ ...available, download_url: "https://example.invalid" }), null);
});

test("model and copy provide a label for every state in Thai and English", () => {
  const states: UpdateState[] = [
    "not_configured", "idle", "checking", "up_to_date", "available", "downloading", "ready", "installing", "failed",
  ];
  for (const state of states) {
    const th = updateView(reply(state), "th");
    const en = updateView(reply(state), "en");
    assert.ok(th.headline.length > 0, `Thai label for ${state}`);
    assert.ok(en.headline.length > 0, `English label for ${state}`);
  }
});

test("available shows version and release notes as text and offers download only", () => {
  const view = updateView(available);
  assert.equal(view.headline, "มีเวอร์ชัน 0.3.1 ใหม่");
  assert.equal(view.detail, "First line\nSecond line");
  assert.equal(view.canDownload, true);
  assert.equal(view.canInstall, false);
  assert.equal(view.canCheck, true);

  const hostileLookingText = "<script>alert(1)</script>\nsecond line";
  assert.equal(updateView({ ...available, available: { ...available.available!, notes: hostileLookingText } }).detail, hostileLookingText);
  const source = readFileSync(new URL("../src/shell/UpdateControl.tsx", import.meta.url), "utf8");
  assert.match(source, /\{view\.detail\}/);
  assert.doesNotMatch(source, /dangerouslySetInnerHTML/);
});

test("progress, ready, and installing have distinct status labels and actions", () => {
  const downloading = updateView(reply("downloading"));
  const ready = updateView(reply("ready"));
  const installing = updateView(reply("installing"));
  assert.equal(downloading.headline, "กำลังดาวน์โหลด 42%");
  assert.equal(downloading.canInstall, false);
  assert.equal(downloading.canDownload, false);
  assert.equal(ready.headline, "ดาวน์โหลดและตรวจสอบแล้ว พร้อมติดตั้ง");
  assert.equal(ready.canInstall, true);
  assert.equal(installing.headline, "กำลังติดตั้งอัปเดตและเริ่มแอปใหม่…");
  assert.equal(installing.canInstall, false);
  assert.equal(updateCopy("en").downloading(42), "Downloading 42%");
});

test("updater unavailable retains the release-page fallback", () => {
  const view = updateView(reply("not_configured", "NO_ENDPOINT"));
  assert.equal(view.canCheck, false);
  assert.equal(view.canDownload, false);
  const source = readFileSync(new URL("../src/shell/UpdateControl.tsx", import.meta.url), "utf8");
  assert.match(source, /onClick=\{\(\) => \{ void openReleasePage\(\); \}\}/);
  assert.match(source, /text\.openPage/);
});

test("match-blocked text replaces available action labels and disables every update action", () => {
  const blocked = updateView(reply("ready", "MATCH_ACTIVE", true));
  assert.equal(blocked.headline, "รอจนจบแมตช์ก่อนอัปเดต");
  assert.equal(blocked.detail, "ไม่อัปเดตระหว่างแมตช์");
  assert.equal(blocked.canCheck, false);
  assert.equal(blocked.canDownload, false);
  assert.equal(blocked.canInstall, false);
  assert.equal(updateView(reply("available", null, true), "en").headline, "Wait until the match ends before updating");
});

test("known failure reasons have localized copy", () => {
  const cases: Array<[UpdateState, string, string]> = [
    ["not_configured", "BUILD_WITHOUT_UPDATER", "บิลด์นี้ไม่รวมตัวอัปเดต"],
    ["not_configured", "NO_ENDPOINT", "ยังไม่มีที่อยู่สำหรับดาวน์โหลดอัปเดต"],
    ["not_configured", "NO_PUBKEY", "ยังไม่มีกุญแจตรวจลายเซ็น"],
    ["failed", "NETWORK", "เชื่อมต่อเพื่อตรวจอัปเดตไม่ได้"],
    ["failed", "SIGNATURE", "ลายเซ็นไม่ถูกต้อง ไม่ติดตั้ง"],
    ["failed", "BACKEND_UNAVAILABLE", "ระบบอัปเดตยังใช้ไม่ได้"],
    ["failed", "BAD_RESPONSE", "ข้อมูลจากระบบอัปเดตไม่ถูกต้อง"],
  ];
  for (const [state, code, detail] of cases) {
    assert.equal(updateView(reply(state, code)).detail, detail, code);
  }
});

test("download and install require two separate user actions", async () => {
  const flow = createUpdateConfirmFlow();
  const calls: string[] = [];
  flow.begin();
  assert.deepEqual(calls, []);
  await flow.confirm(async () => { calls.push("install"); });
  assert.deepEqual(calls, ["install"]);
  flow.begin();
  flow.cancel();
  await flow.confirm(async () => { calls.push("unexpected"); });
  assert.deepEqual(calls, ["install"]);

  const source = readFileSync(new URL("../src/shell/UpdateControl.tsx", import.meta.url), "utf8");
  assert.match(source, /onClick=\{\(\) => \{ void snapshot\.download\?\.\(\); \}\}/);
  assert.match(source, /onClick=\{beginConfirmation\}/);
  assert.match(source, /onClick=\{confirmInstallation\}/);
  assert.match(source, /aria-describedby=\{!canInstall \? "shell-update-disabled-reason"/);
  const runtime = readFileSync(new URL("../src/runtime.ts", import.meta.url), "utf8");
  assert.match(runtime, /"update_download"/);
  assert.match(runtime, /command === "update_download"\) return invoke<unknown>\(command\)/);
  assert.match(runtime, /command === "update_install"\) return invoke<unknown>\(command, \{ confirm: true \}\)/);
});

test("automatic checking defaults off, runs at most once, and never downloads", async () => {
  const calls: string[] = [];
  const store = createUpdateStatusStore(async (command) => {
    calls.push(command);
    return command === "update_check" ? available : reply("idle");
  });
  assert.equal(store.getSnapshot().automaticChecksEnabled, false);
  await store.mount();
  await store.setAutomaticChecksEnabled(true);
  await store.setAutomaticChecksEnabled(false);
  await store.setAutomaticChecksEnabled(true);
  assert.deepEqual(calls, ["update_status", "update_check"]);
  assert.equal(calls.includes("update_install"), false);
  assert.equal(store.getSnapshot().reply?.state, "available");
});

test("status store checks only when called and preserves the parsed reply", async () => {
  const calls: string[] = [];
  const store = createUpdateStatusStore(async (command) => {
    calls.push(command);
    return command === "update_check" ? available : reply("idle");
  });
  await store.mount();
  assert.deepEqual(calls, ["update_status"]);
  await store.check();
  assert.deepEqual(calls, ["update_status", "update_check"]);
  assert.equal(store.getSnapshot().reply?.state, "available");
});

test("status store separates the download click from the confirmed install click", async () => {
  const calls: Array<[string, Record<string, unknown> | undefined]> = [];
  let current = available;
  const store = createUpdateStatusStore(async (command, args) => {
    calls.push([command, args]);
    if (command === "update_download") {
      current = reply("ready");
      return current;
    }
    if (command === "update_install") {
      current = reply("installing");
      return current;
    }
    return current;
  });
  await store.mount();
  await store.download();
  assert.equal(store.getSnapshot().reply?.state, "ready");
  await store.install();
  assert.deepEqual(calls, [
    ["update_status", undefined],
    ["update_download", undefined],
    ["update_install", { confirm: true }],
  ]);
  assert.equal(store.getSnapshot().reply?.state, "installing");
});

test("request errors keep the previous reply and do not expose backend details", async () => {
  let fail = false;
  const store = createUpdateStatusStore(async (command) => {
    if (fail && command === "update_check") throw new Error("private backend details");
    return reply("up_to_date");
  });
  await store.mount();
  const previous = store.getSnapshot().reply;
  fail = true;
  await assert.doesNotReject(store.check());
  assert.deepEqual(store.getSnapshot().reply, previous);
  assert.equal(store.getSnapshot().error, true);
});

test("mock scenarios cover updater states with separate download and install commands", async () => {
  const scenarios = ["available", "failed", "up_to_date", "checking", "downloading", "ready", "installing", "match_blocked"] as const;
  for (const scenario of scenarios) assert.equal(parseMockUpdateScenario(scenario), scenario);
  assert.equal(parseMockUpdateScenario(null), null);
  assert.equal(parseMockUpdateScenario("installed"), null);

  const defaultMock = createMockUpdater(null);
  assert.deepEqual((await defaultMock.updateStatus()).reason, "BUILD_WITHOUT_UPDATER");
  const mock = createMockUpdater("available");
  assert.equal((await mock.updateStatus()).state, "available");
  await assert.rejects(mock.updateInstall({ confirm: false }));
  assert.equal((await mock.updateInstall({ confirm: true })).state, "available");
  assert.equal((await mock.updateDownload()).state, "ready");
  assert.equal((await mock.updateInstall({ confirm: true })).state, "installing");
});

test("browser preview preserves its existing updater command interception", () => {
  const main = readFileSync(new URL("../src/main.tsx", import.meta.url), "utf8");
  const harness = readFileSync(new URL("../src/mock/harness.tsx", import.meta.url), "utf8");
  assert.match(main, /params\.get\("mockUpdate"\)/);
  assert.match(main, /params\.get\("mock"\) === "1"/);
  assert.match(harness, /command === "update_status"/);
  assert.match(harness, /command === "update_check"/);
  assert.match(harness, /command === "update_download"/);
  assert.match(harness, /command === "update_install"/);
});
