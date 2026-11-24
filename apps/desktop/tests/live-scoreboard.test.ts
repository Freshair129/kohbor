import assert from "node:assert/strict";
import { test } from "node:test";

const scoreboard = await import("../src/live/scoreboardModel.ts").catch(() => null);

test("scoreboard model formats known and unknown scores and signed game clocks", () => {
  assert.ok(scoreboard, "scoreboard model is implemented");
  assert.equal(scoreboard.formatScore(null), "—");
  assert.equal(scoreboard.formatScore(0), "0");
  assert.equal(scoreboard.formatGameClock(null), "—");
  assert.equal(scoreboard.formatGameClock(-65), "-01:05");
  assert.equal(scoreboard.formatGameClock(125), "02:05");
});

test("kill momentum stays empty until a full three-minute score window is observed", () => {
  assert.ok(scoreboard, "scoreboard model is implemented");
  assert.equal(scoreboard.killMomentum([], 180_000), null);
  assert.equal(scoreboard.killMomentum([{ atMs: 1, radiant: 2, dire: 1 }], 180_000), null);
  assert.equal(scoreboard.killMomentum([
    { atMs: 1_000, radiant: 2, dire: 1 },
    { atMs: 179_999, radiant: 3, dire: 1 },
  ], 180_000), null);
  assert.equal(scoreboard.killMomentum([
    { atMs: 0, radiant: 2, dire: 1 },
    { atMs: 176_000, radiant: 3, dire: 1 },
  ], 180_000), null, "a baseline older than the three-minute window is not relabelled as last-three-minute data");
});

test("kill momentum leans toward the team with more observed kills in the last three minutes", () => {
  assert.ok(scoreboard, "scoreboard model is implemented");
  const start = { atMs: 0, radiant: 8, dire: 7 };
  assert.deepEqual(scoreboard.killMomentum([start, { atMs: 180_000, radiant: 11, dire: 8 }], 180_000), {
    radiantKills: 3, direKills: 1, radiantPercent: 75, leaning: "radiant",
  });
  assert.deepEqual(scoreboard.killMomentum([start, { atMs: 180_000, radiant: 9, dire: 12 }], 180_000), {
    radiantKills: 1, direKills: 5, radiantPercent: 16.7, leaning: "dire",
  });
});

test("zero kills in a complete window is a neutral empty bar and score resets are unknown", () => {
  assert.ok(scoreboard, "scoreboard model is implemented");
  assert.deepEqual(scoreboard.killMomentum([
    { atMs: 0, radiant: 8, dire: 7 },
    { atMs: 180_000, radiant: 8, dire: 7 },
  ], 180_000), { radiantKills: 0, direKills: 0, radiantPercent: null, leaning: "neutral" });
  assert.equal(scoreboard.killMomentum([
    { atMs: 0, radiant: 8, dire: 7 },
    { atMs: 180_000, radiant: 1, dire: 1 },
  ], 180_000), null);
});
