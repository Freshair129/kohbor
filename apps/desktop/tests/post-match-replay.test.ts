import assert from "node:assert/strict";
import { test } from "node:test";
import { canOpenPostMatchReplay } from "../src/replay/admission.ts";
import { readPostMatchReplay, REPORT_FORMAT } from "../src/replay/report.ts";
import type { RuntimeStatus } from "../src/types.ts";

const noGame = { dota_running: false, game_phase: "none", gsi: "waiting", launch_pending: false } as const;
const postGame = { dota_running: true, game_phase: "post_game", gsi: "receiving", launch_pending: false } as const;

test("preview is admitted with Dota closed or a corroborated post-game phase", () => {
  const confirmed = { session: { phase: "post_game" }, intake: { state: "receiving" } } as Pick<RuntimeStatus, "session" | "intake">;
  assert.equal(canOpenPostMatchReplay(null, noGame, false), true);
  assert.equal(canOpenPostMatchReplay(confirmed, postGame, false), true);
  assert.equal(canOpenPostMatchReplay(null, postGame, false), false);
  assert.equal(canOpenPostMatchReplay(confirmed, { ...postGame, game_phase: "live" }, false), false);
  assert.equal(canOpenPostMatchReplay(confirmed, { ...postGame, gsi: "waiting" }, false), false);
  assert.equal(canOpenPostMatchReplay(confirmed, { ...postGame, launch_pending: true }, false), false);
  assert.equal(canOpenPostMatchReplay(confirmed, postGame, true), false);
  assert.equal(canOpenPostMatchReplay(null, null, false), false);
});

function report() {
  const points = [[0, 0, 0], [1, 100, 200]];
  const roster = [
    { key: "self", hero: "Silencer", team: "Radiant", lifeEnds: [] },
    ...Array.from({ length: 4 }, (_, i) => ({ key: `ally-${i + 1}`, hero: `Ally ${i + 1}`, team: "Radiant", points, lifeEnds: [] })),
    ...Array.from({ length: 5 }, (_, i) => ({ key: `enemy-${i + 1}`, hero: `Enemy ${i + 1}`, team: "Dire", points, lifeEnds: [] })),
  ];
  const calibration = { sx: .05, sy: -.05, ox: 500, oy: 480 };
  return { format: REPORT_FORMAT, source: "DEM",
    maps: [{ patch: "7.41f", width: 1000, height: 959, calibration, map: "data:image/png;base64,YQ==" }],
    matches: [{ patch: "7.41f", id: "1", hero: "Silencer", team: "Radiant", start: 0, duration: 100, points, deaths: [], roster }] };
}
test("report admits a shaped local DEM bundle and rejects duplicate matches or external map URLs", () => {
  const baseline = report();
  assert.equal(readPostMatchReplay(baseline)?.matches.length, 1);
  assert.equal(readPostMatchReplay({ ...baseline, matches: [...baseline.matches, ...baseline.matches] }), null);
  const unsafeMap = { ...baseline.maps[0], map: "https://example.com/map.png" };
  assert.equal(readPostMatchReplay({ ...baseline, maps: [unsafeMap] }), null);
  assert.equal(readPostMatchReplay({ ...baseline, source: "GSI" }), null);
  assert.equal(readPostMatchReplay({ ...baseline, playerAccount: "private" }), null);
});

test("match patch must resolve to one layout and each layout has its own projection", () => {
  const baseline = report();
  const oldMap = baseline.maps[0]!;
  const newMap = { ...oldMap, patch: "7.42", height: 1000, calibration: { ...oldMap.calibration, oy: 500 } };
  const newMatch = { ...baseline.matches[0]!, patch: "7.42", id: "2" };
  const both = { ...baseline, maps: [oldMap, newMap], matches: [...baseline.matches, newMatch] };
  assert.deepEqual(readPostMatchReplay(both)?.maps.map(map => [map.patch, map.height, map.calibration.oy]),
    [["7.41f", 959, 480], ["7.42", 1000, 500]]);
  assert.equal(readPostMatchReplay({ ...both, maps: [oldMap] }), null);
  assert.equal(readPostMatchReplay({ ...both, maps: [oldMap, newMap, newMap] }), null);
  assert.equal(readPostMatchReplay({ ...both, maps: [oldMap, { ...newMap, map: null }] }), null);
});

test("unversioned reports retain their original layout with an unknown patch", () => {
  const baseline = report();
  const { patch: _patch, start: _start, ...game } = baseline.matches[0]!;
  const v1 = { format: "kohbor.post-match-replay.v1", source: "DEM", matches: [game],
    map: baseline.maps[0]!.map, calibration: baseline.maps[0]!.calibration };
  const v2 = readPostMatchReplay(v1);
  assert.equal(v2?.matches[0]?.patch, null);
  assert.equal(v2?.maps[0]?.patch, null);
  assert.equal(v2?.maps[0]?.height, 959);
  assert.equal(v2?.maps[0]?.map, v1.map);
});

test("reports retain observed pre-horn coordinates and exact negative death times", () => {
  const baseline = report();
  const game = baseline.matches[0]!;
  game.start = -40;
  game.points.unshift([-39, 15, 45]);
  const deaths = [{ t: -16.833, x: 15, y: 45, tick: 5834, killer: null }];
  const candidate = { ...baseline, matches: [{ ...game, deaths }] };
  assert.equal(readPostMatchReplay(candidate)?.matches[0]?.deaths[0]?.t, -16.833);
  game.start = 0;
  assert.equal(readPostMatchReplay({ ...candidate, matches: [{ ...game, deaths }] }), null);
});

test("report rejects broken roster ownership and unordered positions", () => {
  const badTeam = report();
  const game = badTeam.matches[0]!;
  game.roster[1]!.team = "Dire";
  assert.equal(readPostMatchReplay(badTeam), null);
  const badClock = report();
  badClock.matches[0]!.points.reverse();
  assert.equal(readPostMatchReplay(badClock), null);
});

test("death review accepts bounded Combat Log entries and rejects future events and unshaped notes", () => {
  const baseline = report();
  const game = baseline.matches[0]!;
  const finish = { actor: "Terrorblade", ability: null, illusion: true };
  const moment = { t: 10.2, kind: "control", actor: "Terrorblade", ability: "Reflection", illusion: false };
  const advice = { th: "ถอยกลับหาเพื่อน", en: "Fall back toward your team." };
  const review = { finish, moments: [moment], summary: null, advice };
  const death = { t: 20, x: 100, y: 200, killer: "Terrorblade", tick: 900, review };
  const candidate = (value: unknown) => ({ ...baseline, matches: [{ ...game, deaths: [value] }] });
  assert.equal(readPostMatchReplay(candidate(death))?.matches[0]?.deaths[0]?.review?.finish.illusion, true);
  for (const badTime of [4.9, 20.01]) {
    assert.equal(readPostMatchReplay(candidate({ ...death, review: { ...review, moments: [{ ...moment, t: badTime }] } })), null);
  }
  assert.equal(readPostMatchReplay(candidate({ ...death, review: { ...review, advice: { th: "ถอย" } } })), null);
  assert.equal(readPostMatchReplay(candidate({ ...death, review: { ...review, moments: [{ ...moment, kind: "other" }] } })), null);
});
