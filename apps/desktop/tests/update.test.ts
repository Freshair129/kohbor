import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { registerHooks } from "node:module";
import { test } from "node:test";
import type { UpdateReply, UpdateState, UpdateView } from "../src/shell/updateModel.ts";

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
const { createUpdateStatusStore } = await import("../src/shell/updateStatusStore.ts");
const { createMockUpdater, parseMockUpdateScenario } = await import("../src/mock/updateHarness.ts");

const available: UpdateReply = {
  state: "available",
  current_version: "0.3.0-dev.15",
  reason: null,
  available: { version: "0.3.1", notes: "First line\nSecond line", pub_date: null },
  match_active: false,
};

function reply(state: UpdateState, reason: string | null = null, match_active = false): UpdateReply {
  return {
    state,
    current_version: "0.3.0-dev.15",
    reason,
    available: state === "available"
      ? { version: "0.3.1", notes: "First line\nSecond line", pub_date: null }
      : null,
    match_active,
  };
}

test("strict parser accepts the complete reply shape", () => {
  assert.deepEqual(parseUpdateReply(available), available);
  assert.deepEqual(parseUpdateReply(reply("not_configured", "BUILD_WITHOUT_UPDATER")), reply("not_configured", "BUILD_WITHOUT_UPDATER"));
});

test("strict parser accepts each declared state", () => {
  const states: UpdateState[] = ["not_configured", "idle", "checking", "up_to_date", "available", "downloading", "ready_to_install", "failed"];
  for (const state of states) assert.equal(parseUpdateReply(reply(state))?.state, state);
});

test("strict parser rejects null, arrays, and primitive top-level values", () => {
  for (const value of [null, [], "reply", 4, true]) assert.equal(parseUpdateReply(value), null);
});

test("strict parser rejects unknown states", () => {
  assert.equal(parseUpdateReply({ ...available, state: "installed" }), null);
});

test("strict parser rejects a non-string current version", () => {
  assert.equal(parseUpdateReply({ ...available, current_version: 4 }), null);
});

test("strict parser rejects a missing current version", () => {
  const { current_version: _currentVersion, ...missing } = available;
  assert.equal(parseUpdateReply(missing), null);
});

test("strict parser rejects non-string available versions", () => {
  assert.equal(parseUpdateReply({ ...available, available: { ...available.available!, version: 7 } }), null);
});

test("strict parser rejects a missing available version", () => {
  const { version: _version, ...missingAvailable } = available.available!;
  assert.equal(parseUpdateReply({ ...available, available: missingAvailable }), null);
});

test("strict parser rejects a non-string notes field", () => {
  assert.equal(parseUpdateReply({ ...available, available: { ...available.available!, notes: null } }), null);
});

test("strict parser rejects a non-string pub_date", () => {
  assert.equal(parseUpdateReply({ ...available, available: { ...available.available!, pub_date: 5 } }), null);
});

test("strict parser requires reason to be a string or null", () => {
  assert.equal(parseUpdateReply({ ...available, reason: false }), null);
});

test("strict parser requires available to be an object or null", () => {
  assert.equal(parseUpdateReply({ ...available, available: "0.3.1" }), null);
});

test("strict parser requires match_active to be boolean", () => {
  assert.equal(parseUpdateReply({ ...available, match_active: "false" }), null);
});

test("strict parser requires every top-level field", () => {
  for (const key of ["state", "reason", "available", "match_active"] as const) {
    const input = { ...available } as Record<string, unknown>;
    delete input[key];
    assert.equal(parseUpdateReply(input), null, `missing ${key}`);
  }
});

test("strict parser rejects unexpected top-level fields", () => {
  assert.equal(parseUpdateReply({ ...available, download_url: "https://example.invalid" }), null);
});

test("unknown reply maps to an explicitly unknown view", () => {
  const view = updateView(null);
  assert.equal(view.headline, "ไม่ทราบสถานะการอัปเดต");
  assert.equal(view.tone, "unknown");
  assert.equal(view.canInstall, false);
});

test("update view for not_configured keeps checks disabled with reason copy", () => {
  const view = updateView(reply("not_configured", "BUILD_WITHOUT_UPDATER"));
  assert.equal(view.headline, "ยังไม่ได้ตั้งค่าการอัปเดตในบิลด์นี้");
  assert.equal(view.detail, "บิลด์นี้ไม่รวมตัวอัปเดต");
  assert.equal(view.canCheck, false);
  assert.equal(view.canInstall, false);
});

test("update view for idle enables manual check", () => {
  const view = updateView(reply("idle"));
  assert.equal(view.headline, "ยังไม่ได้ตรวจ");
  assert.equal(view.canCheck, true);
  assert.equal(view.canInstall, false);
});

test("update view for checking disables both actions", () => {
  const view = updateView(reply("checking"));
  assert.equal(view.headline, "กำลังตรวจหาอัปเดต…");
  assert.equal(view.canCheck, false);
  assert.equal(view.canInstall, false);
});

test("update view for up_to_date says latest only for that reply state", () => {
  const view = updateView(reply("up_to_date"));
  assert.equal(view.headline, "เป็นเวอร์ชันล่าสุดแล้ว");
  assert.equal(view.tone, "positive");
  assert.equal(view.canInstall, false);
});

test("update view for available uses the offered version and notes", () => {
  const view = updateView(available);
  assert.equal(view.headline, "มีเวอร์ชัน 0.3.1 ใหม่");
  assert.equal(view.detail, "First line\nSecond line");
  assert.equal(view.canInstall, true);
  assert.equal(view.canCheck, true);
});

test("available notes remain text and are rendered through React text interpolation", () => {
  const hostileLookingText = "<script>alert(1)</script>\nsecond line";
  const view = updateView({ ...available, available: { ...available.available!, notes: hostileLookingText } });
  assert.equal(view.detail, hostileLookingText);
  const source = readFileSync(new URL("../src/shell/UpdateControl.tsx", import.meta.url), "utf8");
  assert.match(source, /\{view\.detail\}/);
  assert.doesNotMatch(source, /dangerouslySetInnerHTML/);
});

test("update view for downloading and ready_to_install never offers restart", () => {
  const downloading = updateView(reply("downloading"));
  const ready = updateView(reply("ready_to_install"));
  assert.equal(downloading.headline, "กำลังดาวน์โหลด…");
  assert.equal(ready.headline, "ดาวน์โหลดเสร็จ รอการยืนยันรีสตาร์ต");
  assert.equal(downloading.canInstall, false);
  assert.equal(ready.canInstall, false);
});

test("update view for failed displays known reason and allows a manual retry", () => {
  const view = updateView(reply("failed", "SIGNATURE"));
  assert.equal(view.headline, "ตรวจอัปเดตไม่สำเร็จ");
  assert.equal(view.detail, "ลายเซ็นไม่ถูกต้อง ไม่ติดตั้ง");
  assert.equal(view.canCheck, true);
  assert.equal(view.canInstall, false);
});

test("every reason code has explicit Thai text and action availability", () => {
  const cases: Array<[UpdateState, string, string, boolean, boolean]> = [
    ["not_configured", "BUILD_WITHOUT_UPDATER", "บิลด์นี้ไม่รวมตัวอัปเดต", false, false],
    ["not_configured", "NO_ENDPOINT", "ยังไม่มีที่อยู่สำหรับดาวน์โหลดอัปเดต", false, false],
    ["not_configured", "NO_PUBKEY", "ยังไม่มีกุญแจตรวจลายเซ็น", false, false],
    ["failed", "NETWORK", "เชื่อมต่อเพื่อตรวจอัปเดตไม่ได้", true, false],
    ["failed", "SIGNATURE", "ลายเซ็นไม่ถูกต้อง ไม่ติดตั้ง", true, false],
    ["failed", "BACKEND_UNAVAILABLE", "ระบบอัปเดตยังใช้ไม่ได้", true, false],
    ["failed", "BAD_RESPONSE", "ข้อมูลจากระบบอัปเดตไม่ถูกต้อง", true, false],
    ["failed", "MATCH_ACTIVE", "ไม่อัปเดตระหว่างแมตช์", false, false],
  ];
  for (const [state, code, detail, canCheck, canInstall] of cases) {
    const view = updateView(reply(state, code));
    assert.equal(view.detail, detail, code);
    assert.equal(view.canCheck, canCheck, code);
    assert.equal(view.canInstall, canInstall, code);
  }
});

test("all declared states have deliberate action and tone mappings", () => {
  const cases: Array<[UpdateState, boolean, boolean, UpdateView["tone"]]> = [
    ["not_configured", false, false, "warning"],
    ["idle", true, false, "neutral"],
    ["checking", false, false, "neutral"],
    ["up_to_date", true, false, "positive"],
    ["available", true, true, "warning"],
    ["downloading", false, false, "neutral"],
    ["ready_to_install", false, false, "positive"],
    ["failed", true, false, "critical"],
  ];
  for (const [state, canCheck, canInstall, tone] of cases) {
    const view = updateView(reply(state));
    assert.equal(view.canCheck, canCheck, state);
    assert.equal(view.canInstall, canInstall, state);
    assert.equal(view.tone, tone, state);
  }
});

test("MATCH_ACTIVE reason uses the same message as the active-match override", () => {
  const view = updateView(reply("failed", "MATCH_ACTIVE"));
  assert.equal(view.detail, "ไม่อัปเดตระหว่างแมตช์");
  assert.equal(view.canCheck, false);
  assert.equal(view.canInstall, false);
});

test("match_active disables check and install in every state", () => {
  for (const state of ["idle", "available", "failed"] as const) {
    const view = updateView(reply(state, null, true));
    assert.equal(view.detail, "ไม่อัปเดตระหว่างแมตช์");
    assert.equal(view.canCheck, false);
    assert.equal(view.canInstall, false);
  }
});

test("install confirmation does not install until confirmed and cancel is inert", async () => {
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
});

test("UpdateControl keeps install behind its first-click prompt and describes disabled buttons", () => {
  const source = readFileSync(new URL("../src/shell/UpdateControl.tsx", import.meta.url), "utf8");
  assert.match(source, /onClick=\{beginConfirmation\}/);
  assert.match(source, /onClick=\{confirmInstallation\}/);
  assert.match(source, /aria-describedby=\{!canInstall \? "shell-update-disabled-reason"/);
  assert.match(source, /aria-describedby=\{!canCheck \? "shell-update-disabled-reason"/);
  assert.doesNotMatch(source, /onClick=\{install\}/);
});

test("useUpdateStatus mount effect reads status and leaves check and install to the user", () => {
  const source = readFileSync(new URL("../src/shell/useUpdateStatus.ts", import.meta.url), "utf8");
  assert.match(source, /void store\.mount\(\)/);
  assert.doesNotMatch(source, /store\.check\(\)|store\.install\(\)/);
});

test("update status store mounts with only update_status and mount is idempotent", async () => {
  const calls: Array<[string, Record<string, unknown> | undefined]> = [];
  const store = createUpdateStatusStore(async (command, args) => {
    calls.push([command, args]);
    return reply("idle");
  });
  assert.deepEqual(calls, []);
  await store.mount();
  await store.mount();
  assert.deepEqual(calls, [["update_status", undefined]]);
  assert.equal(store.getSnapshot().reply?.state, "idle");
});

test("update status store checks only when called and replaces state with the reply", async () => {
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

test("update install requires available state and sends confirm true", async () => {
  const calls: Array<[string, Record<string, unknown> | undefined]> = [];
  const store = createUpdateStatusStore(async (command, args) => {
    calls.push([command, args]);
    return command === "update_install" ? reply("ready_to_install") : available;
  });
  await store.mount();
  assert.deepEqual(calls, [["update_status", undefined]]);
  await store.install();
  assert.deepEqual(calls, [["update_status", undefined], ["update_install", { confirm: true }]]);
  assert.equal(store.getSnapshot().reply?.state, "ready_to_install");
});

test("update command errors keep the previous reply and expose only short failure state", async () => {
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

test("mock update query accepts only supported explicit scenarios", () => {
  for (const scenario of ["available", "failed", "up_to_date", "downloading"] as const) {
    assert.equal(parseMockUpdateScenario(scenario), scenario);
  }
  assert.equal(parseMockUpdateScenario(null), null);
  assert.equal(parseMockUpdateScenario("installed"), null);
});

test("mock updater defaults to not_configured and rounds available install through confirmed ready state", async () => {
  const defaultMock = createMockUpdater(null);
  assert.deepEqual((await defaultMock.updateStatus()).reason, "BUILD_WITHOUT_UPDATER");

  const mock = createMockUpdater("available");
  assert.equal((await mock.updateStatus()).state, "available");
  assert.equal((await mock.updateCheck()).state, "available");
  await assert.rejects(mock.updateInstall({ confirm: false }));
  assert.equal((await mock.updateInstall({ confirm: true })).state, "ready_to_install");
});

test("browser preview wires explicit update query scenarios without removing the existing mock flag", () => {
  const main = readFileSync(new URL("../src/main.tsx", import.meta.url), "utf8");
  const harness = readFileSync(new URL("../src/mock/harness.tsx", import.meta.url), "utf8");
  assert.match(main, /params\.get\("mockUpdate"\)/);
  assert.match(main, /params\.get\("mock"\) === "1"/);
  assert.match(harness, /command === "update_status"/);
  assert.match(harness, /command === "update_check"/);
  assert.match(harness, /command === "update_install"/);
});
