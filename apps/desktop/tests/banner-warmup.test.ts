import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { readFileSync } from "node:fs";
import { test } from "node:test";
import { startBannerWarmup, WARMUP_MAX_MS, WARMUP_STYLE, type WarmupDeps } from "../src/bannerWarmup.ts";

const ASSET = new URL("../src/assets/overlay/banner-warmup.webm", import.meta.url);
const ASSET_SHA256 = "b61694603c69e768296d48bc2128ce81216d679268ed0c9973b86f1d26a6025d";

function fakeDeps(play: () => Promise<void> = () => Promise.resolve()) {
  const listeners = new Map<string, () => void>();
  const log: string[] = [];
  const attrs = new Map<string, string>();
  const timers: { callback: () => void; ms: number; cleared: boolean }[] = [];
  const video = {
    muted: false, playsInline: false, preload: "", src: "", style: { cssText: "" },
    setAttribute: (name: string, value: string) => { attrs.set(name, value); },
    removeAttribute: (name: string) => { log.push(`removeAttribute:${name}`); },
    addEventListener: (type: string, listener: () => void) => { listeners.set(type, listener); },
    play: () => { log.push("play"); return play(); },
    pause: () => { log.push("pause"); },
    load: () => { log.push("load"); },
    remove: () => { log.push("remove"); },
  };
  const deps: WarmupDeps = {
    createVideo: () => video,
    append: () => { log.push("append"); },
    setTimer: (callback, ms) => { const t = { callback, ms, cleared: false }; timers.push(t); return t; },
    clearTimer: (handle) => { (handle as { cleared: boolean }).cleared = true; },
  };
  return { deps, video, listeners, log, attrs, timers };
}

test("the warm-up plays one muted, hidden, 1x1 px video and bounds it with a timer", () => {
  const f = fakeDeps();
  startBannerWarmup("/warmup.webm", f.deps);
  assert.equal(f.video.src, "/warmup.webm");
  assert.equal(f.video.muted, true);
  assert.equal(f.video.playsInline, true);
  assert.equal(f.attrs.get("aria-hidden"), "true");
  assert.equal(f.video.style.cssText, WARMUP_STYLE);
  assert.match(WARMUP_STYLE, /width:1px;height:1px;opacity:0\.05;pointer-events:none/);
  assert.deepEqual(f.log, ["append", "play"]);
  assert.equal(f.timers.length, 1);
  assert.equal(f.timers[0]!.ms, WARMUP_MAX_MS);
});

test("the warm-up removes itself when the clip ends and stop stays idempotent", () => {
  const f = fakeDeps();
  const stop = startBannerWarmup("/warmup.webm", f.deps);
  f.listeners.get("ended")!();
  assert.deepEqual(f.log.slice(2), ["pause", "removeAttribute:src", "load", "remove"]);
  assert.equal(f.timers[0]!.cleared, true);
  stop();
  f.timers[0]!.callback();
  assert.equal(f.log.filter((entry) => entry === "remove").length, 1);
});

test("an error, a refused play or the timer each end the warm-up", async () => {
  const onError = fakeDeps();
  startBannerWarmup("/warmup.webm", onError.deps);
  onError.listeners.get("error")!();
  assert.ok(onError.log.includes("remove"));

  const refused = fakeDeps(() => Promise.reject(new Error("NotAllowedError")));
  startBannerWarmup("/warmup.webm", refused.deps);
  await new Promise((resolve) => setImmediate(resolve));
  assert.ok(refused.log.includes("remove"));

  const slow = fakeDeps(() => new Promise(() => {}));
  startBannerWarmup("/warmup.webm", slow.deps);
  slow.timers[0]!.callback();
  assert.ok(slow.log.includes("remove"));
});

test("without a document the warm-up does nothing", () => {
  const stop = startBannerWarmup("/warmup.webm", null);
  assert.doesNotThrow(stop);
});

test("the bundled warm-up clip is the small original VP9 alpha WebM", () => {
  const bytes = readFileSync(ASSET);
  assert.equal(createHash("sha256").update(bytes).digest("hex"), ASSET_SHA256);
  assert.ok(bytes.length < 32 * 1024, `warm-up clip is ${bytes.length} bytes`);
  // Above Vite's default 4 KiB assetsInlineLimit, so it ships as a file served from 'self'; a data: URL
  // would be refused by media-src.
  assert.ok(bytes.length > 4096, `warm-up clip is ${bytes.length} bytes and would be inlined as data:`);
  assert.deepEqual([...bytes.subarray(0, 4)], [0x1a, 0x45, 0xdf, 0xa3]);
  assert.ok(bytes.includes(Buffer.from("webm", "ascii")));
  assert.ok(bytes.includes(Buffer.from("V_VP9", "ascii")));
  // AlphaMode (0x53C0) = 1 and BlockAdditions (0x75A1) carry the alpha stream.
  assert.ok(bytes.includes(Buffer.from([0x53, 0xc0, 0x81, 0x01])));
  assert.ok(bytes.includes(Buffer.from([0x75, 0xa1])));
});

test("the overlay starts the warm-up after the host's first status and stops it when a banner starts", () => {
  const main = readFileSync(new URL("../src/main.tsx", import.meta.url), "utf8");
  assert.match(main, /import bannerWarmupUrl from "\.\/assets\/overlay\/banner-warmup\.webm";/);
  assert.match(main, /setHostStatusSeen\(true\);/);
  assert.match(main, /if \(!hostStatusSeen \|\| stopBannerWarmup\.current \|\| currentVisualId\.current\) return;\s*stopBannerWarmup\.current = startBannerWarmup\(bannerWarmupUrl\);/);
  assert.match(main, /if \(visualId\) stopBannerWarmup\.current\?\.\(\);/);
});
