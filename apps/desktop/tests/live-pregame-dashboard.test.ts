import assert from "node:assert/strict";
import { test } from "node:test";
import { allModesScopeLabel, EMPTY_HISTORY, type HistoryEntry, type MatchHistory } from "../src/overview/overviewModel.ts";

const dashboard = await import("../src/live/pregameDashboardModel.ts").catch(() => null);
const historyWith = (entries: HistoryEntry[]): MatchHistory => ({ ...EMPTY_HISTORY, entries, recorded: entries.length });

test("pre-game dashboard shows honest empty and unavailable history states", () => {
  assert.ok(dashboard, "pre-game dashboard model is implemented");
  assert.equal(dashboard.pregameDashboardModel(null, new Date(2026, 10, 24, 12), "th").state, "unavailable");
  const empty = dashboard.pregameDashboardModel(EMPTY_HISTORY, new Date(2026, 10, 24, 12), "th");
  assert.equal(empty.state, "empty");
  assert.equal(empty.emptyMessage, "ยังไม่มีแมตช์ที่บันทึก");
  assert.equal(empty.lastMatch, null);
  assert.equal(empty.bestHeroThisWeek, null);
});

test("pre-game dashboard summarizes local matches today and yesterday with all-modes scope", () => {
  assert.ok(dashboard, "pre-game dashboard model is implemented");
  const now = new Date(2026, 10, 24, 12);
  const history = historyWith([
    { matchKey: "today-1", source: "kohbor", hero: "npc_dota_hero_axe", result: "win", end: "completed", kills: 4, deaths: 1, assists: 8, durationS: 1800, endedAt: Math.floor(new Date(2026, 10, 24, 11).getTime() / 1000), matchType: "unknown" },
    { matchKey: "today-2", source: "kohbor", hero: "npc_dota_hero_axe", result: "win", end: "completed", kills: 7, deaths: 2, assists: 5, durationS: 2100, endedAt: Math.floor(new Date(2026, 10, 24, 10).getTime() / 1000), matchType: "unknown" },
    { matchKey: "today-3", source: "kohbor", hero: "npc_dota_hero_lina", result: "loss", end: "completed", kills: 2, deaths: 5, assists: 6, durationS: 1500, endedAt: Math.floor(new Date(2026, 10, 24, 9).getTime() / 1000), matchType: "unknown" },
    { matchKey: "yesterday", source: "kohbor", hero: "npc_dota_hero_axe", result: "win", end: "completed", kills: 5, deaths: 3, assists: 7, durationS: 1900, endedAt: Math.floor(new Date(2026, 10, 23, 20).getTime() / 1000), matchType: "unknown" },
    { matchKey: "provider-only", source: "opendota", hero: "npc_dota_hero_axe", result: "win", end: "completed", kills: 99, deaths: 0, assists: 0, durationS: 1000, endedAt: Math.floor(new Date(2026, 10, 24, 8).getTime() / 1000), matchType: "ranked" },
  ]);
  const view = dashboard.pregameDashboardModel(history, now, "en");
  assert.equal(view.state, "ready");
  assert.deepEqual(view.today, { matches: 3, wins: 2, losses: 1, noResult: 0, winRate: 66.7 });
  assert.deepEqual(view.yesterday, { matches: 1, wins: 1, losses: 0, noResult: 0, winRate: 100 });
  assert.equal(view.scopeLabel, allModesScopeLabel("en"));
  assert.deepEqual(view.lastMatch, {
    hero: "npc_dota_hero_axe", heroName: "Axe", result: "win", kda: "4 / 1 / 8", duration: "30:00",
  });
  assert.deepEqual(view.bestHeroThisWeek, { hero: "npc_dota_hero_axe", heroName: "Axe", games: 3, wins: 3, losses: 0, winRate: 100 });
});
