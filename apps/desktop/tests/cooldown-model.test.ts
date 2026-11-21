import assert from "node:assert/strict";
import { test } from "node:test";

async function loadModel() {
  try {
    return await import("../src/deck/cooldownModel.ts");
  } catch {
    assert.fail("cooldownModel.ts must provide the cooldown tracking model");
  }
}

test("integer reports keep whole-second display while fractional reports use at most one decimal", async () => {
  const { formatCooldownValue, trackCooldown } = await loadModel();
  const integer = trackCooldown(null, { remaining: 12 }, 0);
  const quarter = trackCooldown(null, { remaining: 0.25 }, 0);
  const tenth = trackCooldown(null, { remaining: 1.2 }, 0);

  assert.equal(integer.display, "12");
  assert.equal(quarter.display, "0.3");
  assert.equal(tenth.display, "1.2");
  assert.equal(formatCooldownValue(0.25), "0.3");
  for (const value of [integer.display, quarter.display, tenth.display]) assert.match(value, /^\d+(?:\.\d)?$/);
});

test("cooldown identity changes when source context or slot occupant changes", async () => {
  const { cooldownTrackKey } = await loadModel();
  const initial = cooldownTrackKey("ability", "[1,2,3,lion]", 0, "lion_impale");
  assert.notEqual(cooldownTrackKey("ability", "[1,2,4,lion]", 0, "lion_impale"), initial);
  assert.notEqual(cooldownTrackKey("ability", "[1,2,3,shadow_shaman]", 0, "lion_impale"), initial);
  assert.notEqual(cooldownTrackKey("ability", "[1,2,3,lion]", 0, "lion_voodoo"), initial);
});

test("denominator tracks the largest remaining report and resets when cooldown is confirmed ready", async () => {
  const { trackCooldown } = await loadModel();
  const first = trackCooldown(null, { remaining: 12 }, 0);
  const increased = trackCooldown(first, { remaining: 15 }, 500);
  const decreased = trackCooldown(increased, { remaining: 5 }, 1000);
  assert.equal(decreased.denominator, 15);

  const ready = trackCooldown(decreased, { remaining: 4, ready: true }, 1500);
  assert.equal(ready.active, false);
  assert.equal(ready.denominator, 0);
});

test("zero or null resets cooldown history", async () => {
  const { trackCooldown } = await loadModel();
  const first = trackCooldown(null, { remaining: 9 }, 0);
  assert.equal(trackCooldown(first, { remaining: 0 }, 1000).active, false);
  assert.equal(trackCooldown(first, { remaining: null }, 1000).active, false);
});

test("estimated remaining clamps at zero without becoming confirmed ready", async () => {
  const { estimateCooldown, trackCooldown } = await loadModel();
  const tracked = trackCooldown(null, { remaining: 2 }, 0);
  const expiredEstimate = estimateCooldown(tracked, 5000);
  assert.equal(expiredEstimate.remaining, 0);
  assert.equal(expiredEstimate.fraction, 0);
  assert.equal(expiredEstimate.display, "0");
  assert.equal(expiredEstimate.active, true);
});

test("a changed source report snaps the countdown to the new value", async () => {
  const { estimateCooldown, trackCooldown } = await loadModel();
  const first = trackCooldown(null, { remaining: 10 }, 0);
  assert.equal(estimateCooldown(first, 1000).remaining, 9);
  const snapped = trackCooldown(first, { remaining: 6 }, 1000);
  assert.equal(estimateCooldown(snapped, 1000).remaining, 6);
});

test("stale sections freeze the current estimate until a fresh report resumes it", async () => {
  const { estimateCooldown, trackCooldown } = await loadModel();
  const first = trackCooldown(null, { remaining: 10 }, 0);
  const stale = trackCooldown(first, { remaining: 10, stale: true }, 2000);
  assert.equal(estimateCooldown(stale, 5000).remaining, 8);
  assert.equal(estimateCooldown(stale, 5000).stale, true);
  const resumed = trackCooldown(stale, { remaining: 5 }, 5000);
  assert.equal(estimateCooldown(resumed, 5000).remaining, 5);
  assert.equal(estimateCooldown(resumed, 5000).stale, false);
});

test("cooldown scheduling is idle without an active timer and respects visibility and reduced motion", async () => {
  const { cooldownTickMode, trackCooldown } = await loadModel();
  const idle = trackCooldown(null, { remaining: null }, 0);
  const active = trackCooldown(null, { remaining: 8 }, 0);
  const stale = trackCooldown(active, { remaining: 8, stale: true }, 0);

  assert.equal(cooldownTickMode([idle], 0, true, false), null);
  assert.equal(cooldownTickMode([active], 0, true, false), "raf");
  assert.equal(cooldownTickMode([active], 0, false, false), null);
  assert.equal(cooldownTickMode([active], 0, true, true), "interval");
  assert.equal(cooldownTickMode([stale], 1000, true, false), null);
  assert.equal(cooldownTickMode([active], 9000, true, false), null);
});

test("a repeated whole-second report keeps its anchor so the estimate keeps running smoothly", async () => {
  const { trackCooldown, estimateCooldown } = await loadModel();
  const first = trackCooldown(null, { remaining: 12 }, 1_000);
  const repeat = trackCooldown(first, { remaining: 12 }, 1_400);
  assert.equal(repeat.reportedAtMs, 1_000, "identical repeats must not re-anchor");
  assert.equal(estimateCooldown(repeat, 1_400).remaining.toFixed(1), "11.6");
  const next = trackCooldown(repeat, { remaining: 11 }, 2_000);
  assert.equal(next.reportedAtMs, 2_000, "a changed value re-anchors");
  assert.equal(next.denominator, 12);
});
