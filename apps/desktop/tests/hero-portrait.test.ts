import assert from "node:assert/strict";
import { test } from "node:test";
import { createHeroPortraitCache, deriveHeroPortraitView, parseHeroPortraitResponse, portraitInitials } from "../src/deck/heroPortraitModel.ts";

// Synthetic non-art bytes: tests exercise the IPC envelope and data-URL mapping, not image decoding.
const base64 = Buffer.from("synthetic portrait response").toString("base64");
const image = { mime: "image/png", base64 };
const src = `data:image/png;base64,${base64}`;

test("monograms use one or two display-name initials including hyphenated names", () => {
  for (const [input, initials] of [["Anti-Mage", "AM"], ["Lion", "L"], ["Shadow Fiend", "SF"],
    ["npc_dota_hero_antimage", "AM"], ["npc_dota_hero_nevermore", "SF"], [null, "—"], ["", "—"]] as const) {
    assert.equal(portraitInitials(input), initials);
  }
});

test("only a ready local PNG response maps to an image; other states keep the monogram", () => {
  assert.deepEqual(parseHeroPortraitResponse(image), { state: "ready", src });
  assert.deepEqual(parseHeroPortraitResponse(null), { state: "none" });
  for (const input of [{ mime: "image/jpeg", base64 }, { mime: "image/png", base64: "https://example.invalid/image" }, { mime: "image/png", base64: "" }, {}]) {
    assert.equal(parseHeroPortraitResponse(input).state, "error");
  }
  for (const state of ["none", "loading", "error"] as const) {
    const view = deriveHeroPortraitView("npc_dota_hero_lion", { state });
    assert.equal(view.state, state); assert.equal(view.initials, "L"); assert.equal(view.src, null);
  }
  assert.equal(deriveHeroPortraitView("npc_dota_hero_lion", { state: "ready", src }).src, src);
  assert.equal(deriveHeroPortraitView(null, { state: "ready", src }).src, null, "old image cannot represent an unknown hero");
});

test("decode failure falls back to initials without attempting to render the failed image again", () => {
  const view = deriveHeroPortraitView("npc_dota_hero_lion", { state: "ready", src }, true);
  assert.deepEqual(view, { state: "error", initials: "L", src: null });
});

test("concurrent portrait requests share one invoke and ready results are cached per hero id", async () => {
  const calls: unknown[] = [];
  let resolve!: (value: unknown) => void;
  const pending = new Promise<unknown>(done => { resolve = done; });
  const cache = createHeroPortraitCache((command, args) => { calls.push({ command, args }); return pending; });
  const first = cache.load("npc_dota_hero_lion");
  const second = cache.load("npc_dota_hero_lion");
  assert.strictEqual(first, second);
  assert.equal(cache.peek("npc_dota_hero_lion")?.state, "loading");
  await Promise.resolve();
  assert.deepEqual(calls, [{ command: "hero_portrait", args: { hero_id: "npc_dota_hero_lion" } }]);
  resolve(image);
  assert.deepEqual(await first, { state: "ready", src });
  assert.strictEqual(await cache.load("npc_dota_hero_lion"), await first);
  assert.equal(calls.length, 1);
  await cache.load("npc_dota_hero_nevermore");
  assert.equal(calls.length, 2);
});

test("missing command rejection resolves to none once and is cached without retrying", async () => {
  let calls = 0;
  const cache = createHeroPortraitCache(async () => { calls += 1; throw new Error("unknown command hero_portrait"); });
  await assert.doesNotReject(async () => assert.deepEqual(await cache.load("npc_dota_hero_lion"), { state: "none" }));
  assert.deepEqual(await cache.load("npc_dota_hero_lion"), { state: "none" });
  assert.equal(calls, 1);
});

test("synchronous invoke errors, null responses and malformed responses never escape or loop", async () => {
  for (const [invoke, expected] of [
    [() => { throw new Error("host absent"); }, "none"],
    [async () => null, "none"], [async () => ({ mime: "image/png" }), "error"],
  ] as const) {
    let calls = 0;
    const cache = createHeroPortraitCache((command, args) => { calls += 1; return invoke(command, args); });
    assert.equal((await cache.load("npc_dota_hero_lion")).state, expected);
    assert.equal((await cache.load("npc_dota_hero_lion")).state, expected);
    assert.equal(calls, 1);
  }
});

test("unknown hero ids do not invoke the host or acquire a portrait", async () => {
  let calls = 0;
  const cache = createHeroPortraitCache(async () => { calls += 1; return null; });
  for (const id of [null, "", "   "]) {
    assert.deepEqual(await cache.load(id), { state: "none" });
    assert.deepEqual(cache.peek(id), { state: "none" });
  }
  assert.equal(calls, 0);
});
