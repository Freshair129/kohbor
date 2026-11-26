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
    recap: { events: [{ event_type: "hero_kill", game_clock_seconds: 180, audio_class: "ordinary", outcome: "admitted" }], droppedCount: 1 },
    derivedStats: [{ label: "Attack range", value: "600" }],
    replay: null,
  }, "en");
  assert.equal(view.result, "win");
  assert.equal(view.duration, "42:00");
  assert.equal(view.heroName, "Axe");
  assert.equal(view.kda, "8 / 2 / 10");
  assert.equal(view.lastHitsDenies, "210 / 13");
  assert.equal(view.gpmXpm, "520 / 610");
  assert.equal(view.finalScore, "Radiant 36 : 28 Dire");
  assert.deepEqual(view.recapEvents, [{ event: "Kill", time: "03:00", kind: "Standard", outcome: "Played" }]);
  assert.equal(view.recapDroppedCount, 1);
  assert.equal(view.recapAvailable, true);
  assert.deepEqual(view.derivedStats, [{ label: "Attack range", value: "600" }]);
  assert.equal(view.replay, null);
});

test("post-game stage one can use fresh GSI values alone but never guesses a result", () => {
  assert.ok(summary, "post-game summary model is implemented");
  const view = summary.postGameSummaryModel({
    entry: null,
    snapshot,
    scoreboard: { radiant: 36, dire: 28, clockSeconds: 2520 },
    recap: null,
    derivedStats: [],
    replay: null,
  }, "en");
  assert.equal(view.result, null);
  assert.equal(view.duration, "42:00");
  assert.equal(view.heroName, "Axe");
  assert.equal(view.kda, "8 / 2 / 10");
  assert.equal(view.lastHitsDenies, "210 / 13");
  assert.equal(view.gpmXpm, "520 / 610");
  assert.equal(view.finalScore, "Radiant 36 : 28 Dire");
  assert.equal(view.recapAvailable, false);
  assert.deepEqual(view.recapEvents, []);
});

test("kills and score differences never determine a missing win or loss", () => {
  assert.ok(summary, "post-game summary model is implemented");
  const input = {
    entry: null,
    snapshot,
    scoreboard: { radiant: 36, dire: 28, clockSeconds: 2520 },
    recap: null,
    derivedStats: [],
    replay: null,
  };
  assert.equal(summary.postGameSummaryModel(input, "en").result, null);
  assert.equal(summary.postGameSummaryModel({ ...input, scoreboard: { ...input.scoreboard, radiant: 23, dire: 33 } }, "en").result, null);
  assert.equal(summary.postGameSummaryModel({ ...input, snapshot: { ...snapshot, own: { ...snapshot.own, team: "dire" } } }, "en").result, null);
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
  assert.equal(absent.result, null);
  assert.equal(absent.duration, "—");
  assert.equal(absent.heroName, "—");
  assert.equal(absent.kda, "— / — / —");
  assert.equal(absent.lastHitsDenies, "— / —");
  assert.equal(absent.gpmXpm, "— / —");
  assert.equal(absent.finalScore, "Radiant — : — Dire");
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

test("a ready replay does not select history when the ended session entry is unknown", () => {
  assert.ok(summary, "post-game summary model is implemented");
  const view = summary.postGameSummaryModel({
    entry: null,
    snapshot: null,
    scoreboard: { radiant: null, dire: null, clockSeconds: null },
    recap: null,
    derivedStats: [],
    replay: { matchKey: entry.matchKey, ready: true, headline: { deaths: 2, netWorth: 18_400 } },
  }, "en");
  assert.equal(view.replay, null);
});

test("a matching replay that is not ready stays in the honest empty state", () => {
  assert.ok(summary, "post-game summary model is implemented");
  const view = summary.postGameSummaryModel({
    entry,
    snapshot: null,
    scoreboard: { radiant: null, dire: null, clockSeconds: null },
    recap: null,
    derivedStats: [],
    replay: { matchKey: entry.matchKey, ready: false, headline: { deaths: null, netWorth: null } },
  }, "en");
  assert.equal(view.replay, null);
});

test("a ready matching replay with unknown headline fields renders dashes", () => {
  assert.ok(summary, "post-game summary model is implemented");
  const view = summary.postGameSummaryModel({
    entry,
    snapshot: null,
    scoreboard: { radiant: null, dire: null, clockSeconds: null },
    recap: null,
    derivedStats: [],
    replay: { matchKey: entry.matchKey, ready: true, headline: { deaths: null, netWorth: null } },
  }, "en");
  assert.deepEqual(view.replay?.headline, [
    { label: "Deaths", value: "—" },
    { label: "Net worth", value: "—" },
  ]);
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

test("last-finished record wins over stale renderer state and carries an explicit watcher unknown", () => {
  assert.ok(summary, "post-game summary model is implemented");
  const record = {
    epoch: 8,
    matchKey: entry.matchKey,
    endedAt: entry.endedAt,
    endValues: { heroId: "npc_dota_hero_axe", result: "loss", kills: 8, deaths: 2, assists: 10,
      lastHits: 210, denies: 13, gpm: 520, xpm: 610, durationSeconds: 2520, radiantScore: 36, direScore: 52 },
  };
  const view = summary.postGameSummaryModel({
    entry: { ...entry, result: "win" }, snapshot: { ...snapshot, availability: "stale" },
    scoreboard: { radiant: 1, dire: 0, clockSeconds: 1 }, recap: null, derivedStats: [],
    replay: { matchKey: entry.matchKey, ready: false, headline: { deaths: null, netWorth: null },
      parseState: "unknown", lastFinishedMatch: record,
      watcher: { state: "unknown", folder: "unknown", lastScanAtMs: null } },
  }, "en");
  assert.equal(view.result, "loss");
  assert.equal(view.kda, "8 / 2 / 10");
  assert.equal(view.finalScore, "Radiant 36 : 52 Dire");
  assert.equal(view.replayParseState, "unknown");
  assert.deepEqual(view.watcher, { state: "unknown", folder: "unknown", lastScanAtMs: null });
});

test("post-game panel explains Dota download, manual picking, and a reachable last-match reopen action", () => {
  const panel = readFileSync(new URL("../src/live/PostGameSummary.tsx", import.meta.url), "utf8");
  const deck = readFileSync(new URL("../src/deck/Deck.tsx", import.meta.url), "utf8");
  const runtime = readFileSync(new URL("../src/runtime.ts", import.meta.url), "utf8");
  assert.match(panel, /stepDownload/);
  assert.match(panel, /<strong>\{text\.watcher\}:<\/strong> \{text\.watcherUnknown\}/);
  assert.match(panel, /onPickReplay/);
  assert.match(panel, /stepOpen/);
  assert.match(deck, /postGameReplay\?\.lastFinishedMatch\?\.epoch === postGame\.epoch/);
  assert.match(deck, /"Last match" : "แมตช์ล่าสุด"/);
  assert.match(runtime, /HIDDEN_STATUS_POLL_MS/);
  assert.match(runtime, /runtime_get_status/);
  const copy = readFileSync(new URL("../src/live/copy.ts", import.meta.url), "utf8");
  assert.match(copy, /stateNotFound: "ยังไม่แยก"/);
  assert.match(copy, /stateQueued: "รอคิว"/);
  assert.match(copy, /stateParsing: "กำลังแยก"/);
  assert.match(copy, /stateReady: "พร้อมดู"/);
  assert.match(copy, /stateFailed: "แยกไม่สำเร็จ"/);
});

test("Deck resolves the ended history entry by the host session key and refreshes on library changes", () => {
  const deck = readFileSync(new URL("../src/deck/Deck.tsx", import.meta.url), "utf8");
  assert.match(deck, /lastLiveSnapshot\?\.epoch === postGame\.epoch \? lastLiveSnapshot\.snapshot : snapshot/);
  assert.match(deck, /history\?\.entries\.find\(entry => entry\.source === "kohbor" && entry\.matchKey === postGameReplay\.matchKey\)/);
  assert.match(deck, /entry: historyEntry/);
  assert.match(deck, /snapshot: summarySnapshot/);
  assert.match(deck, /replay: postGameReplay/);
  assert.match(deck, /post_game_replay_summary/);
  assert.match(deck, /sessionEpoch: null/);
  assert.match(deck, /replay-library:\/\/state-changed/);
  assert.match(deck, /recap: recapMatch \? \{ events: recapMatch\.playedEvents, droppedCount: recapMatch\.droppedCount \} : null/);
});
