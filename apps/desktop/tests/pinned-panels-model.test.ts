import assert from "node:assert/strict";
import { test } from "node:test";
// @ts-ignore Node's built-in TypeScript loader requires the explicit source extension.
import {
  buildDamageLog,
  buildFarmSeries,
  buildKillTimeline,
  deriveAgentOrbMotion,
  deriveObjectiveTimerRows,
  deriveRoshanOverlay,
  canShowRoshanDeathControl,
  type LiveMatchSample,
} from "../src/live/pinnedPanelsModel.ts";
// @ts-ignore Node's built-in TypeScript loader requires the explicit source extension.
import * as PinnedPanelsModelApi from "../src/live/pinnedPanelsModel.ts";
// @ts-ignore Node's built-in TypeScript loader requires the explicit source extension.
import { countdownLabel, parseObjectiveReadModel, parseObjectiveSchedule, RUNE_SPOT_PROVENANCE, RUNE_SPOTS, spotLabels } from "../src/live/objectiveCountdownModel.ts";

const sample = (sequence: number, atMs: number, gameSeconds: number | null, hp: number | null,
  gpm: number | null = null, xpm: number | null = null, lastHits: number | null = null): LiveMatchSample =>
  ({ sequence, atMs, gameSeconds, hp, gpm, xpm, lastHits });

test("own-hero damage log merges drops within 500 ms, omits heals, and keeps unknown values unknown", () => {
  const samples = [
    sample(1, 0, 10, 500),
    sample(2, 100, 10, 490),
    sample(3, 450, 11, 480),
    sample(4, 800, 12, 520), // healing is kept for the sparkline, not listed as damage
    sample(5, 1_400, 13, 470),
    sample(6, 1_700, 14, null),
    sample(7, 2_000, null, 460),
  ];
  const rows = buildDamageLog(samples);
  assert.deepEqual(rows.map(row => [row.hpLost, row.hpAfter, row.gameSeconds]), [[50, 470, 13], [20, 480, 10]]);
  assert.equal(buildDamageLog([sample(1, 0, null, null)]).length, 0);
});

test("farm graph uses only known own samples and marks the last observed LH at or before 10:00", () => {
  const series = buildFarmSeries([
    sample(1, 0, 0, null, 80, 90, 0),
    sample(2, 1_000, 590, null, 240, 310, 54),
    sample(3, 2_000, 603, null, 250, 322, 57),
    sample(4, 3_000, 620, null, null, 330, null),
  ]);
  assert.deepEqual(series?.points.map(point => point.gameSeconds), [0, 590, 603, 620]);
  assert.deepEqual(series?.lhAt10, { gameSeconds: 600, lastHits: 54 });
  assert.deepEqual(series?.latest, { gpm: 250, xpm: 330, lastHits: 57 });
  assert.equal(buildFarmSeries([sample(1, 0, 620, null, null, null, null)])?.lhAt10, null);
  assert.equal(buildFarmSeries([sample(1, 0, 590, null, 200, 240, 50)])?.lhAt10, null, "no marker before the game clock passes 10:00");
});

test("kill timeline keeps the scoreboard's Radiant and Dire samples on game time", () => {
  const timeline = buildKillTimeline([
    { atMs: 1, gameSeconds: 30, radiant: 0, dire: 1 },
    { atMs: 2, gameSeconds: 90, radiant: 2, dire: 1 },
    { atMs: 3, radiant: 3, dire: 2 },
  ]);
  assert.deepEqual(timeline, [
    { gameSeconds: 30, radiant: 0, dire: 1 },
    { gameSeconds: 90, radiant: 2, dire: 1 },
  ]);
  assert.deepEqual(buildKillTimeline([]), []);
});

test("objective timers use the accepted ALERT-008 patch, GSI observations, unknown values, and ready state", () => {
  const schedule = parseObjectiveSchedule({
    schema_version: 1,
    rule_source: "ALERT-008",
    patch_version: "7.42",
    observed_clock_seconds: 450,
    upcoming: [
      { objective: "power_rune", occurrence_seconds: 480, lead_s: [30, 15] },
      { objective: "tormentor", occurrence_seconds: 900, lead_s: [] },
    ],
  });
  assert.ok(schedule);
  const mapFields = {
    clock_time: { value: 450, availability: "fresh" as const },
    daytime: { value: true, availability: "fresh" as const },
    nightstalker_night: { value: null, availability: "unknown" as const },
    roshan_state: { value: "alive", availability: "fresh" as const },
    roshan_respawn_seconds: { value: 42, availability: "fresh" as const },
  };
  const known = deriveObjectiveTimerRows(schedule, 1_000, 1_100, 450, mapFields, "fresh");
  assert.deepEqual(known.find(row => row.id === "power_rune"), { id: "power_rune", state: "countdown", text: "00:30", patchVersion: "7.42" });
  assert.deepEqual(known.find(row => row.id === "tormentor"), { id: "tormentor", state: "countdown", text: "07:30", patchVersion: "7.42" });
  assert.deepEqual(known.find(row => row.id === "lotus"), { id: "lotus", state: "unknown", text: "— ไม่ทราบ", patchVersion: null });
  assert.deepEqual(known.find(row => row.id === "day_night"), { id: "day_night", state: "observed", text: "กลางวัน · สลับ — ไม่ทราบ", patchVersion: null });
  assert.deepEqual(known.find(row => row.id === "roshan"), { id: "roshan", state: "countdown", text: "00:42", patchVersion: null });
  assert.deepEqual(known.find(row => row.id === "roshan_switch"), { id: "roshan_switch", state: "unknown", text: "— ไม่ทราบ", patchVersion: null });
  const justSpawned = parseObjectiveSchedule({ ...schedule, observed_clock_seconds: 482, upcoming: [
    { objective: "power_rune", occurrence_seconds: 600, last_occurrence_seconds: 480, lead_s: [] },
  ] });
  assert.deepEqual(deriveObjectiveTimerRows(justSpawned, 1_000, 1_100, 482, mapFields, "fresh").find(row => row.id === "power_rune"),
    { id: "power_rune", state: "spawned", text: "", patchVersion: "7.42" });
  assert.deepEqual(deriveObjectiveTimerRows(schedule, 1_000, 7_000, 450, mapFields, "fresh").find(row => row.id === "power_rune"),
    { id: "power_rune", state: "unknown", text: "— ไม่ทราบ", patchVersion: "7.42" }, "stale schedules become unknown while retaining the rule patch label");
  const fixturePitRule = {
    patchVersion: "fixture-only", patchVerified: true, cycleStartSeconds: 0,
    daySeconds: 300, nightSeconds: 300, dayPit: "radiant" as const, nightPit: "dire" as const,
  };
  const boundaryFields = { ...mapFields,
    clock_time: { value: 299, availability: "fresh" as const },
    daytime: { value: true, availability: "fresh" as const },
  };
  assert.deepEqual(deriveObjectiveTimerRows(schedule, 1_000, 1_100, 299, boundaryFields, "fresh", "th", fixturePitRule).find(row => row.id === "roshan_switch"),
    { id: "roshan_switch", state: "countdown", text: "ย้ายใน 0:01", patchVersion: "fixture-only" });
  const staleDaytimeFields = { ...boundaryFields, daytime: { value: null, availability: "stale" as const } };
  assert.deepEqual(deriveObjectiveTimerRows(schedule, 1_000, 1_100, 299, staleDaytimeFields, "fresh", "th", fixturePitRule).find(row => row.id === "roshan_switch"),
    { id: "roshan_switch", state: "unknown", text: "— ไม่ทราบ", patchVersion: null }, "stale GSI daytime never advances a switch timer");
  const unknown = deriveObjectiveTimerRows(schedule, 1_000, 1_100, 450, null, "fresh");
  assert.deepEqual(unknown.find(row => row.id === "day_night"), { id: "day_night", state: "unknown", text: "— ไม่ทราบ", patchVersion: null });
  assert.deepEqual(unknown.find(row => row.id === "roshan"), { id: "roshan", state: "unknown", text: "— ไม่ทราบ", patchVersion: null });
  assert.deepEqual(unknown.find(row => row.id === "roshan_switch"), { id: "roshan_switch", state: "unknown", text: "— ไม่ทราบ", patchVersion: null });
  assert.ok(deriveObjectiveTimerRows(schedule, 1_000, 1_100, 450, mapFields, "stale").every(row => row.state === "unknown"));
  assert.deepEqual(countdownLabel({ objective: "power_rune", occurrence_seconds: 480, last_occurrence_seconds: 480, lead_s: [] }, 480),
    { objective: "power_rune", text: null, mode: "spawned", remaining: 0, patch_version: null });
  assert.deepEqual(countdownLabel({ objective: "power_rune", occurrence_seconds: 600, last_occurrence_seconds: 480, lead_s: [] }, 483)?.mode, "spawned");
  assert.equal(countdownLabel({ objective: "power_rune", occurrence_seconds: 480, last_occurrence_seconds: 480, lead_s: [] }, 484), null,
    "the full-colour spawned pulse is brief and does not use wall-clock extrapolation");
});

test("map markers use replay-normalised coordinates with recorded patch/build provenance", () => {
  const labels = [
    { objective: "power_rune" as const, text: "60", mode: "seconds" as const, remaining: 60, patch_version: "7.42" },
    { objective: "bounty_rune" as const, text: "55", mode: "seconds" as const, remaining: 55, patch_version: "7.42" },
  ];
  const shared = spotLabels(labels).find(spot => spot.x === RUNE_SPOTS.power_rune[0].x && spot.y === RUNE_SPOTS.power_rune[0].y);
  assert.deepEqual([RUNE_SPOTS.power_rune[0].x, RUNE_SPOTS.power_rune[0].y], [0.4154139453125, 0.441105375]);
  assert.deepEqual(shared?.labels.map(label => label.objective), ["bounty_rune", "power_rune"], "co-located runes keep both countdowns");
  assert.deepEqual(RUNE_SPOTS.bounty_rune.map(({ x, y }) => [x, y]), [
    [0.4154139453125, 0.441105375], [0.44659700390625, 0.28193598828125],
    [0.5236346533203124, 0.717913359375], [0.55196087890625, 0.55274934375],
  ]);
  assert.equal(RUNE_SPOTS.wisdom_rune.length, 0);
  assert.equal(RUNE_SPOTS.lotus.length, 0);
  assert.deepEqual([RUNE_SPOT_PROVENANCE.patch, RUNE_SPOT_PROVENANCE.gameBuild, RUNE_SPOT_PROVENANCE.replayBundles], ["7.41f", null, 3]);
  assert.deepEqual(spotLabels(labels, { power_rune: [{ x: 1.2, y: -0.1 }] }, true), []);
  assert.deepEqual(spotLabels(labels, RUNE_SPOTS, false), []);
});

test("objective read-model parser preserves direct GSI day/night and Roshan fields while unknown stays null", () => {
  const parsed = parseObjectiveReadModel({
    schema_version: 1,
    availability: "fresh",
    source: { generation: 1, sequence: 5, age_ms: 25 },
    schedule: { schema_version: 1, rule_source: "ALERT-008", patch_version: "7.42", observed_clock_seconds: 90,
      upcoming: [{ objective: "power_rune", occurrence_seconds: 360, lead_s: [] }] },
    map: {
      clock_time: { value: 90, availability: "fresh" }, daytime: { value: false, availability: "fresh" },
      nightstalker_night: { value: null, availability: "unknown" }, roshan_state: { value: "alive", availability: "fresh" },
      roshan_respawn_seconds: { value: 42, availability: "fresh" },
    },
  });
  assert.equal(parsed?.schedule.rule_source, "ALERT-008");
  assert.deepEqual(parsed?.map.daytime, { value: false, availability: "fresh" });
  assert.deepEqual(parsed?.map.roshan_respawn_seconds, { value: 42, availability: "fresh" });
  assert.equal(parseObjectiveReadModel({ ...parsed, map: { ...parsed?.map, daytime: { value: "night", availability: "fresh" } } }), null);
});

test("Roshan pit remains unknown without an admitted pit field or verified rule", () => {
  assert.deepEqual(deriveRoshanOverlay(null), { state: "unknown", text: "— ไม่ทราบ", changed: false });
  assert.deepEqual(deriveRoshanOverlay({ source: "patch-rule", patchVerified: false, pit: "radiant", changed: true }),
    { state: "unknown", text: "— ไม่ทราบ", changed: false });
  const known = deriveRoshanOverlay({ source: "gsi", patchVerified: true, pit: "dire", changed: true });
  assert.deepEqual(known, { state: "known", text: "Dire", changed: true });
  assert.equal(canShowRoshanDeathControl(null), false);
  assert.equal(canShowRoshanDeathControl({ minSeconds: 480, maxSeconds: 660, patchVerified: false }), false);
  assert.equal(canShowRoshanDeathControl({ minSeconds: 480, maxSeconds: 660, patchVerified: true }), true);
});

test("Roshan switch countdown follows an explicit day/night rule across the boundary and stays unknown without one", () => {
  const api = PinnedPanelsModelApi as unknown as Record<string, unknown>;
  const derive = api.deriveRoshanSwitchCountdown as (clockSeconds: number | null, daytime: boolean | null, rule: Record<string, unknown> | null) => unknown;
  assert.equal(typeof derive, "function", "Roshan switch countdown logic is part of the pure model");
  const fixtureRule = {
    patchVersion: "fixture-only",
    patchVerified: true,
    cycleStartSeconds: 0,
    daySeconds: 300,
    nightSeconds: 300,
    dayPit: "radiant",
    nightPit: "dire",
  };
  assert.deepEqual(derive(299, true, fixtureRule), {
    state: "countdown", secondsUntilSwitch: 1, targetPit: "dire", patchVersion: "fixture-only",
  });
  assert.deepEqual(derive(300, false, fixtureRule), {
    state: "countdown", secondsUntilSwitch: 300, targetPit: "radiant", patchVersion: "fixture-only",
  });
  assert.deepEqual(derive(299, true, null), {
    state: "unknown", secondsUntilSwitch: null, targetPit: null, patchVersion: null,
  });
  assert.deepEqual(derive(299, false, fixtureRule), {
    state: "unknown", secondsUntilSwitch: null, targetPit: null, patchVersion: null,
  }, "a GSI day/night state that contradicts the recorded cycle stays unknown");
});

test("agent orb reports real output level when supplied, playback activity when no meter exists, and respects reduced motion", () => {
  assert.deepEqual(deriveAgentOrbMotion({ outputLevel: 0.42, playbackActive: true, reducedMotion: false, visible: true }),
    { source: "level", level: 0.42, motion: "playing", animate: true });
  assert.deepEqual(deriveAgentOrbMotion({ outputLevel: null, playbackActive: true, reducedMotion: false, visible: true }),
    { source: "playback", level: null, motion: "playing", animate: true });
  assert.deepEqual(deriveAgentOrbMotion({ outputLevel: 0.8, playbackActive: true, reducedMotion: true, visible: true }),
    { source: "level", level: 0.8, motion: "static", animate: false });
  assert.equal(deriveAgentOrbMotion({ outputLevel: null, playbackActive: true, reducedMotion: false, visible: false }).animate, false);
});
