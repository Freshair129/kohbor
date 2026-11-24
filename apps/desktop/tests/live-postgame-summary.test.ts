import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { test } from "node:test";

const summary = await import("../src/live/postGameSummaryModel.ts").catch(() => null);

const entry = {
  matchKey: "a".repeat(64), source: "kohbor", hero: "npc_dota_hero_axe", result: "win", end: "completed",
  kills: 8, deaths: 2, assists: 10, durationS: 2520, endedAt: 1_800_000_000, matchType: "unknown",
} as const;
const field = (value: unknown) => ({ value, availability: value === null ? "unknown" : "fresh" });
const snapshot = {
  availability: "fresh",
  own: { team: "radiant", fields: {
    hero_id: field("npc_dota_hero_axe"), kills: field(8), deaths: field(2), assists: field(10),
    last_hits: field(210), denies: field(13), gpm: field(520), xpm: field(610),
  } },
};

test("post-game stage one summarizes GSI and local match history with no replay claim", () => {
  assert.ok(summary, "post-game summary model is implemented");
  const view = summary.postGameSummaryModel({
    entry,
    snapshot,
    scoreboard: { radiant: 36, dire: 28, clockSeconds: 2520 },
    recap: { events: [{ event_type: "tower_kill", game_clock_seconds: 180, audio_class: "notice", outcome: "admitted" }], droppedCount: 1 },
    derivedStats: [{ label: "Attack range", value: "600" }],
    replay: null,
  }, "en");
  assert.equal(view.result, "win");
  assert.equal(view.duration, "42:00");
  assert.equal(view.heroName, "Axe");
  assert.equal(view.kda, "8 / 2 / 10");
  assert.equal(view.lastHitsDenies, "210 / 13");
  assert.equal(view.gpmXpm, "520 / 610");
  assert.equal(view.finalScore, "36 — 28");
  assert.deepEqual(view.recapEvents, [{ event: "tower_kill", time: "03:00", kind: "notice", outcome: "admitted" }]);
  assert.equal(view.recapDroppedCount, 1);
  assert.equal(view.recapAvailable, true);
  assert.deepEqual(view.derivedStats, [{ label: "Attack range", value: "600" }]);
  assert.equal(view.replay, null);
});

test("post-game stage one can use fresh GSI values alone without inventing recap data", () => {
  assert.ok(summary, "post-game summary model is implemented");
  const view = summary.postGameSummaryModel({
    entry: null,
    snapshot,
    scoreboard: { radiant: 36, dire: 28, clockSeconds: 2520 },
    recap: null,
    derivedStats: [],
    replay: null,
  }, "en");
  assert.equal(view.result, "win");
  assert.equal(view.duration, "42:00");
  assert.equal(view.heroName, "Axe");
  assert.equal(view.kda, "8 / 2 / 10");
  assert.equal(view.lastHitsDenies, "210 / 13");
  assert.equal(view.gpmXpm, "520 / 610");
  assert.equal(view.finalScore, "36 — 28");
  assert.equal(view.recapAvailable, false);
  assert.deepEqual(view.recapEvents, []);
});

test("GSI result follows our admitted team and separate final scores; ties stay unknown", () => {
  assert.ok(summary, "post-game summary model is implemented");
  const input = {
    entry: null,
    snapshot,
    scoreboard: { radiant: 36, dire: 28, clockSeconds: 2520 },
    recap: null,
    derivedStats: [],
    replay: null,
  };
  assert.equal(summary.postGameSummaryModel({ ...input, snapshot: { ...snapshot, own: { ...snapshot.own, team: "dire" } } }, "en").result, "loss");
  assert.equal(summary.postGameSummaryModel({ ...input, scoreboard: { ...input.scoreboard, dire: 36 } }, "en").result, "—");
  assert.equal(summary.postGameSummaryModel({ ...input, snapshot: { ...snapshot, own: { ...snapshot.own, team: null } } }, "en").result, "—");
});

test("post-game unknown values stay as dashes and a ready replay is linked only by the matching install hash", () => {
  assert.ok(summary, "post-game summary model is implemented");
  const base = {
    entry: { ...entry, result: null, hero: null, kills: null, deaths: null, assists: null, durationS: null },
    snapshot: null,
    scoreboard: { radiant: null, dire: null, clockSeconds: null },
    recap: { events: [], droppedCount: 0 },
    derivedStats: [],
  };
  const absent = summary.postGameSummaryModel({ ...base, replay: null }, "th");
  assert.equal(absent.result, "—");
  assert.equal(absent.duration, "—");
  assert.equal(absent.heroName, "—");
  assert.equal(absent.kda, "— / — / —");
  assert.equal(absent.lastHitsDenies, "— / —");
  assert.equal(absent.gpmXpm, "— / —");
  assert.equal(absent.finalScore, "— — —");
  assert.equal(absent.recapAvailable, true);
  assert.equal(absent.replay, null);

  const ready = summary.postGameSummaryModel({ ...base, replay: {
    matchKey: entry.matchKey,
    ready: true,
    headline: { deaths: 2, netWorth: 18_400 },
  } }, "en");
  assert.deepEqual(ready.replay, {
    headline: [{ label: "Deaths", value: "2" }, { label: "Net worth", value: "18,400" }],
    openMatchKey: entry.matchKey,
  });
  const wrongMatch = summary.postGameSummaryModel({ ...base, replay: {
    matchKey: "b".repeat(64), ready: true, headline: { deaths: 2, netWorth: 18_400 },
  } }, "en");
  assert.equal(wrongMatch.replay, null);
});

test("post-game snapshot values must still be fresh to appear in the summary", () => {
  assert.ok(summary, "post-game summary model is implemented");
  const stale = { ...snapshot, availability: "stale" };
  const view = summary.postGameSummaryModel({
    entry: null,
    snapshot: stale,
    scoreboard: { radiant: null, dire: null, clockSeconds: null },
    recap: null,
    derivedStats: [],
    replay: null,
  }, "en");
  assert.equal(view.heroName, "—");
  assert.equal(view.kda, "— / — / —");
  assert.equal(view.recapAvailable, false);
});

test("Deck freezes the current session's fresh GSI snapshot for stage one and keeps replay unlinked", () => {
  const deck = readFileSync(new URL("../src/deck/Deck.tsx", import.meta.url), "utf8");
  assert.match(deck, /lastLiveSnapshot\?\.epoch === postGame\.epoch \? lastLiveSnapshot\.snapshot : snapshot/);
  assert.match(deck, /entry: null/);
  assert.match(deck, /snapshot: summarySnapshot/);
  assert.match(deck, /replay: null/);
  assert.match(deck, /recap: recapMatch \? \{ events: recapMatch\.playedEvents, droppedCount: recapMatch\.droppedCount \} : null/);
});
