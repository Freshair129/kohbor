import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { test } from "node:test";
import { ratio, summarize } from "../src/replay/coach.ts";
import { readPostMatchReplay, REPORT_FORMAT_V3 } from "../src/replay/report.ts";

// Built by tools/replay_export from a trimmed, DEM-derived neutral model with the
// real match ID replaced; tools/tests/test_replay_export.py keeps it in sync.
const raw = readFileSync(new URL("./fixtures/replay/report-v3-fixture.json", import.meta.url), "utf8");
const fixture = () => JSON.parse(raw);

test("v3 Death review report from the exporter is admitted", () => {
  const report = readPostMatchReplay(fixture());
  assert.equal(report?.format, REPORT_FORMAT_V3);
  assert.equal(report?.rules?.provisional, true);
  assert.equal(report?.rules?.enemyPositions, "full-map");
  const coach = report?.matches[0]?.coach;
  assert.deepEqual(coach?.deaths.map(d => d.type), ["solo", "other", "teamfight", "teamfight", "other", "other"]);
  assert.ok((coach?.notes.length ?? 9) <= 3);
});

test("v3 rejects unshaped coach data, mismatched deaths, unknown types and stray personal fields", () => {
  const mutate = (change: (r: any) => void) => { const r = fixture(); change(r); return readPostMatchReplay(r); };
  assert.equal(mutate(r => { delete r.matches[0].coach; }), null);
  assert.equal(mutate(r => { delete r.rules; }), null);
  assert.equal(mutate(r => { r.rules.provisional = false; }), null);
  assert.equal(mutate(r => { r.rules.enemyPositions = "visible"; }), null);
  assert.equal(mutate(r => { r.matches[0].coach.deaths[0].tick += 1; }), null);
  assert.equal(mutate(r => { r.matches[0].coach.deaths.pop(); }), null);
  assert.equal(mutate(r => { r.matches[0].coach.deaths[0].type = "unlucky"; }), null);
  assert.equal(mutate(r => { r.matches[0].coach.deaths[0].player_name = "x"; }), null);
  assert.equal(mutate(r => { r.matches[0].roster[1].account_id = 1; }), null);
  assert.equal(mutate(r => { r.matches[0].coach.notes[0].evidence = [1]; }), null);
  assert.equal(mutate(r => { r.matches[0].coach.notes.push(...r.matches[0].coach.notes); }), null);
  assert.equal(mutate(r => { r.matches[0].coach.lane10.lanes[0].heroes = ["enemy-1"]; }), null);
  assert.equal(mutate(r => { r.format = "kohbor.post-match-replay.v2"; }), null);
  // A v2 file still opens without coach data.
  assert.notEqual(mutate(r => { r.format = "kohbor.post-match-replay.v2"; delete r.rules; delete r.matches[0].coach; }), null);
});

test("fixture holds no SteamID-shaped numbers or personal keys", () => {
  assert.doesNotMatch(raw, /7656119\d{10}/);
  assert.doesNotMatch(raw, /"(player_?name|persona(name)?|steam_?id|account_?id|chat|name)"\s*:/i);
});

test("summary counts deaths as k/n below ten matches and adds rates from ten", () => {
  const report = readPostMatchReplay(fixture())!;
  const s = summarize(report)!;
  assert.equal(s.matches, 1);
  assert.deepEqual(s.byType, { solo: 1, gank: 0, teamfight: 2, other: 3 });
  assert.deepEqual(s.byPhase.map(p => p.solo + p.gank + p.teamfight + p.other), [2, 0, 1, 3]);
  assert.deepEqual(s.lanes, [{ lane: "mid", matches: 1, behind: 1, even: 0, ahead: 0, unknown: 0, medianDiff: -717 }]);
  assert.equal(ratio(s.byType.other, s.deaths.length, s.matches), "3/6");
  assert.equal(ratio(3, 6, 10), "3/6 (50%)");
  assert.equal(summarize({ ...report, rules: undefined }), null);
});
