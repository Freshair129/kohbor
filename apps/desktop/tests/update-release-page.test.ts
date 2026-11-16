import assert from "node:assert/strict";
import { registerHooks } from "node:module";
import test from "node:test";

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

const { parseReleasePageReply, releasePageMessage } = await import("../src/shell/updateModel.ts");
const { createUpdateStatusStore } = await import("../src/shell/updateStatusStore.ts");

const NOT_CONFIGURED = {
  state: "not_configured",
  current_version: "0.3.0-dev.15",
  reason: "BUILD_WITHOUT_UPDATER",
  available: null,
  match_active: false,
};

function recorder(replies: Record<string, unknown>) {
  const calls: string[] = [];
  const invoke = async (command: string) => {
    calls.push(command);
    return replies[command];
  };
  return { calls, invoke };
}

test("release page reply parser is strict", () => {
  assert.deepEqual(
    parseReleasePageReply({ opened: true, reason: null, url: "https://github.com/Freshair129/kohbor/releases/latest" }),
    { opened: true, reason: null, url: "https://github.com/Freshair129/kohbor/releases/latest" },
  );
  assert.deepEqual(parseReleasePageReply({ opened: false, reason: "MATCH_ACTIVE", url: null }), { opened: false, reason: "MATCH_ACTIVE", url: null });
  for (const bad of [null, [], {}, { opened: "yes", reason: null, url: null }, { opened: true, reason: 1, url: null }, { opened: true, reason: null, url: 5 }, { opened: true, reason: null, url: null, extra: 1 }]) {
    assert.equal(parseReleasePageReply(bad), null, JSON.stringify(bad));
  }
});

test("release page messages: opened, match refusal, any other failure, nothing yet", () => {
  assert.equal(releasePageMessage(null, "en"), null);
  assert.match(releasePageMessage({ opened: true, reason: null, url: "u" }, "en") ?? "", /Opened/);
  assert.match(releasePageMessage({ opened: false, reason: "MATCH_ACTIVE", url: null }, "en") ?? "", /during a match/);
  assert.match(releasePageMessage({ opened: false, reason: "OPEN_FAILED", url: "u" }, "en") ?? "", /Could not open/);
  assert.match(releasePageMessage({ opened: true, reason: null, url: "u" }, "th") ?? "", /เปิดหน้า/);
});

test("store: mounting only reads the status, opening the page is an explicit user action", async () => {
  const { calls, invoke } = recorder({
    update_status: NOT_CONFIGURED,
    update_open_release_page: { opened: true, reason: null, url: "https://github.com/Freshair129/kohbor/releases/latest" },
  });
  const store = createUpdateStatusStore(invoke as never);
  await store.mount();
  assert.deepEqual(calls, ["update_status"], "no automatic check or page open on mount");
  await store.openReleasePage();
  assert.deepEqual(calls, ["update_status", "update_open_release_page"]);
  assert.deepEqual(store.getSnapshot().page, { opened: true, reason: null, url: "https://github.com/Freshair129/kohbor/releases/latest" });
  assert.equal(store.getSnapshot().pending, null);
});

test("store: a live match blocks opening the page without any invoke", async () => {
  const { calls, invoke } = recorder({ update_status: { ...NOT_CONFIGURED, match_active: true } });
  const store = createUpdateStatusStore(invoke as never);
  await store.mount();
  await store.openReleasePage();
  assert.deepEqual(calls, ["update_status"]);
});

test("store: a failing or malformed reply keeps the last status and flags an error", async () => {
  const failing = createUpdateStatusStore((async (command: string) => {
    if (command === "update_status") return NOT_CONFIGURED;
    throw new Error("boom");
  }) as never);
  await failing.mount();
  await failing.openReleasePage();
  assert.equal(failing.getSnapshot().error, true);
  assert.equal(failing.getSnapshot().reply?.state, "not_configured");
  const malformed = createUpdateStatusStore((async (command: string) => (command === "update_status" ? NOT_CONFIGURED : { nope: 1 })) as never);
  await malformed.mount();
  await malformed.openReleasePage();
  assert.equal(malformed.getSnapshot().error, true);
  assert.equal(malformed.getSnapshot().page, null);
});

test("store: only one request at a time", async () => {
  let release: (() => void) | undefined;
  const gate = new Promise<void>((resolve) => { release = resolve; });
  const calls: string[] = [];
  const store = createUpdateStatusStore((async (command: string) => {
    calls.push(command);
    if (command === "update_status") return NOT_CONFIGURED;
    await gate;
    return { opened: true, reason: null, url: null };
  }) as never);
  await store.mount();
  const first = store.openReleasePage();
  await store.openReleasePage();
  release?.();
  await first;
  assert.equal(calls.filter((c) => c === "update_open_release_page").length, 1);
});
