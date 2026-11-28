import assert from "node:assert/strict";
import test from "node:test";
import { buildOpponentsViewModel } from "../src/analytics/knowledge/opponentsModel.mjs";

const ownHeroId = "npc_dota_hero_axe";
const enemyIds = ["npc_dota_hero_lina", "npc_dota_hero_drow_ranger"];
const realShapedMatchData = () => ({
  heroes: [
    { id: ownHeroId, name: "Axe", team: 2, pid: 0 },
    { id: "npc_dota_hero_tidehunter", name: "Tidehunter", team: 2, pid: 1 },
    { id: enemyIds[0], name: "Lina", team: 3, pid: 5 },
    { id: enemyIds[1], name: "Drow Ranger", team: 3, pid: 6 },
  ],
  deaths: [
    { t: 10, hero: enemyIds[0], killer: ownHeroId },
    { t: 20, hero: enemyIds[1], killer: ownHeroId },
    { t: 30, hero: ownHeroId, killer: enemyIds[0] },
    { t: 40, hero: ownHeroId, killer: "npc_dota_goodguys_tower1_top" },
    { t: 50, hero: ownHeroId, killer: "npc_dota_hero_tidehunter" },
    { t: 60, hero: ownHeroId, killer: null },
  ],
});

const completeDigestStats = () => ({
  kill_gold_by_victim: {
    complete: true,
    complete_by_player_hero_id: { [ownHeroId]: true },
    by_player_hero_id: { [ownHeroId]: { [enemyIds[0]]: 320, [enemyIds[1]]: 320 } },
  },
  gold_lost_by_killer: {
    complete: true,
    complete_by_player_hero_id: { [ownHeroId]: true },
    by_player_hero_id: { [ownHeroId]: { [enemyIds[0]]: 310, [enemyIds[1]]: 0 } },
    other_by_player_hero_id: { [ownHeroId]: 55 },
  },
  neutral_kill_gold_by_camp: null,
});

test("opponent model attributes only enemy hero events and preserves ties and other deaths", () => {
  const matchData = realShapedMatchData();
  const view = buildOpponentsViewModel({ matchData, digest: { opponent_stats: completeDigestStats() }, ownHeroId });
  assert.deepEqual(view.enemyRows.map(row => [row.heroId, row.kills, row.deaths, row.killGold, row.goldLost]), [
    [enemyIds[0], 1, 1, 320, 310],
    [enemyIds[1], 1, 0, 320, 0],
  ]);
  assert.equal(view.otherDeaths, 2);
  assert.equal(view.unknownKillerDeaths, 1);
  assert.deepEqual(view.headlines.mostKills.heroIds, enemyIds);
  assert.deepEqual(view.headlines.mostKillGold.heroIds, enemyIds);
  assert.deepEqual(view.headlines.mostDeaths.names, ["Lina"]);
  assert.deepEqual(view.headlines.mostGoldLost.names, ["Lina"]);
  assert.equal(view.otherGoldLost, 55);
  assert.equal(view.headlines.mostJungleGold.state, "unknown");
});

test("jungle gold selects every tied zone and retains unattached-count context", () => {
  const stats = completeDigestStats();
  stats.neutral_kill_gold_by_camp = {
    complete: true,
    complete_by_player_hero_id: { [ownHeroId]: true },
    by_player_hero_id: { [ownHeroId]: { R1: 200, R2: 200, D3: 75 } },
    zone_names: { R1: "ป่า Radiant 1 (R1)", R2: "ป่า Radiant 2 (R2)", D3: "ป่า Dire 3 (D3)" },
    unattached_by_player_hero_id: { [ownHeroId]: 2 },
  };
  const view = buildOpponentsViewModel({ matchData: realShapedMatchData(), digest: { opponent_stats: stats }, ownHeroId });
  assert.deepEqual(view.headlines.mostJungleGold.zoneIds, ["R1", "R2"]);
  assert.deepEqual(view.headlines.mostJungleGold.names, ["ป่า Radiant 1 (R1)", "ป่า Radiant 2 (R2)"]);
  assert.equal(view.headlines.mostJungleGold.gold, 200);
  assert.equal(view.jungleUnattached, 2);
});

test("an empty mapped jungle-zone set remains unknown", () => {
  const stats = completeDigestStats();
  stats.neutral_kill_gold_by_camp = {
    complete: true,
    complete_by_player_hero_id: { [ownHeroId]: true },
    by_player_hero_id: { [ownHeroId]: {} },
    zone_names: {},
    unattached_by_player_hero_id: { [ownHeroId]: 1 },
  };
  const view = buildOpponentsViewModel({ matchData: realShapedMatchData(), digest: { opponent_stats: stats }, ownHeroId });
  assert.equal(view.headlines.mostJungleGold.state, "unknown");
  assert.equal(view.jungleUnattached, 1);
});

test("zero events say none, missing events stay unknown, and absent engine fields never estimate", () => {
  const zero = buildOpponentsViewModel({ matchData: { ...realShapedMatchData(), deaths: [] }, digest: { opponent_stats: completeDigestStats() }, ownHeroId });
  assert.equal(zero.headlines.mostKills.state, "none");
  assert.equal(zero.headlines.mostDeaths.state, "none");
  assert.equal(zero.enemyRows[0].kills, 0);
  assert.equal(zero.enemyRows[0].deaths, 0);
  assert.equal(zero.headlines.mostKillGold.state, "known");

  const missing = buildOpponentsViewModel({ matchData: { heroes: realShapedMatchData().heroes }, ownHeroId });
  assert.equal(missing.enemyRows[0].kills, null);
  assert.equal(missing.otherDeaths, null);
  assert.equal(missing.headlines.mostKills.state, "unknown");

  const digestWithoutDeathEvidence = {
    heroes: realShapedMatchData().heroes,
    deaths: [],
    deaths_reason: "death events are missing from the bundle",
  };
  const digestMissing = buildOpponentsViewModel({ digest: digestWithoutDeathEvidence, ownHeroId });
  assert.equal(digestMissing.enemyRows[0].kills, null);
  assert.equal(digestMissing.otherDeaths, null);
  assert.equal(digestMissing.headlines.mostDeaths.state, "unknown");

  const oldBundle = {
    heroes: realShapedMatchData().heroes,
    deaths: [],
    opponent_stats: completeDigestStats(),
  };
  const old = buildOpponentsViewModel({ matchData: oldBundle, ownHeroId });
  for (const key of ["mostKillGold", "mostGoldLost", "mostJungleGold"]) {
    assert.equal(old.headlines[key].state, "unknown");
  }
  assert.equal(old.enemyRows[0].killGold, null);
  assert.equal(old.enemyRows[0].goldLost, null);
});

test("incomplete or malformed gold aggregates stay unknown instead of filling zero", () => {
  const matchData = realShapedMatchData();
  const digestStats = completeDigestStats();
  digestStats.kill_gold_by_victim.complete = false;
  digestStats.kill_gold_by_victim.complete_by_player_hero_id[ownHeroId] = false;
  digestStats.kill_gold_by_victim.by_player_hero_id[ownHeroId] = null;
  digestStats.gold_lost_by_killer.complete_by_player_hero_id[ownHeroId] = false;
  digestStats.gold_lost_by_killer.by_player_hero_id[ownHeroId] = null;
  digestStats.gold_lost_by_killer.other_by_player_hero_id[ownHeroId] = null;
  const view = buildOpponentsViewModel({ matchData, digest: { opponent_stats: digestStats }, ownHeroId });
  assert.equal(view.enemyRows[0].killGold, null);
  assert.equal(view.headlines.mostKillGold.state, "unknown");
  assert.equal(view.enemyRows[0].goldLost, null);
  assert.equal(view.headlines.mostGoldLost.state, "unknown");

  digestStats.kill_gold_by_victim.complete = true;
  digestStats.kill_gold_by_victim.complete_by_player_hero_id[ownHeroId] = true;
  digestStats.kill_gold_by_victim.by_player_hero_id[ownHeroId] = { [enemyIds[0]]: 0.5, [enemyIds[1]]: 320 };
  const fractional = buildOpponentsViewModel({ matchData, digest: { opponent_stats: digestStats }, ownHeroId });
  assert.equal(fractional.enemyRows[0].killGold, null);
  assert.equal(fractional.headlines.mostKillGold.state, "unknown");
});

test("a hero killer with unknown team is not assigned to enemy or other deaths", () => {
  const matchData = realShapedMatchData();
  matchData.heroes.push({ id: "npc_dota_hero_pudge", name: "Pudge", pid: 8 });
  matchData.deaths = [{ t: 1, hero: ownHeroId, killer: "npc_dota_hero_pudge", killer_kind: "hero" }];
  const view = buildOpponentsViewModel({ matchData, ownHeroId });
  assert.equal(view.otherDeaths, 0);
  assert.equal(view.unknownKillerDeaths, 1);
  assert.equal(view.headlines.mostDeaths.state, "none");
});
