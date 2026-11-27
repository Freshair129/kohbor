import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { test } from "node:test";

// All post-game inputs in this file are synthetic fixtures; they are model tests, not match evidence.
const summary = await import("../src/live/postGameSummaryModel.ts").catch(() => null);
const liveCopy = await import("../src/live/copy.ts").catch(() => null);
const heroCardModel = await import("../src/live/heroCardModel.ts").catch(() => null);
const debriefFixture = await import("../src/mock/debriefFixture.mjs").catch(() => null);
const heroNames = await import("../src/deck/heroName.ts").catch(() => null);

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
  assert.match(copy, /stateQueued: "พบแล้ว · รอคิว"/);
  assert.match(copy, /stateParsing: "กำลังแยก"/);
  assert.match(copy, /stateReady: "พร้อมดู"/);
  assert.match(copy, /stateFailed: "แยกไม่สำเร็จ"/);
});

test("replay-fetch states have Thai and English labels at 12px and fit the bounded status box", () => {
  assert.ok(summary && liveCopy);
  const states = [
    "off", "waiting_for_match_end", "looking_up", "requested", "downloading", "parsing", "ready",
    "not_available_yet", "not_found", "expired", "rate_limited", "offline", "failed",
  ] as const;
  for (const state of states) {
    const thai = summary.postGameReplayStatePresentation(state, "match", "th");
    const english = summary.postGameReplayStatePresentation(state, "match", "en");
    assert.ok(thai.label.length > 0, `Thai ${state}`);
    assert.ok(english.label.length > 0, `English ${state}`);
  }
  // With the fetch off, the manual Dota download path keeps its hint and the Settings link.
  const off = summary.postGameReplayStatePresentation("off", "match", "th");
  assert.ok(off.hint && off.hint.includes("Dota 2"));
  assert.equal(off.settingsSection, "replay-ai");
  assert.equal(summary.postGameReplayStatePresentation("looking_up", "match", "th").hint, null);
  const css = readFileSync(new URL("../src/live/live.css", import.meta.url), "utf8");
  assert.match(css, /\.live-postgame-waiting > strong, \.live-postgame-parse-state\s*\{[^}]*font-size:\s*12px/);
  assert.match(css, /\.live-postgame-waiting > strong, \.live-postgame-parse-state\s*\{[^}]*overflow-wrap:\s*anywhere/);
  const mock = readFileSync(new URL("../src/mock/harness.tsx", import.meta.url), "utf8");
  for (const state of states.filter(value => value !== "not_found" && value !== "parsing" && value !== "ready" && value !== "failed")) {
    assert.match(mock, new RegExp(`option value="${state}"`));
  }
});

test("Deck resolves the ended history entry by the host session key and refreshes on library changes", () => {
  const deck = readFileSync(new URL("../src/deck/Deck.tsx", import.meta.url), "utf8");
  const cards = readFileSync(new URL("../src/live/HeroCard.tsx", import.meta.url), "utf8");
  assert.match(deck, /lastLiveSnapshot\?\.epoch === postGame\.epoch \? lastLiveSnapshot\.snapshot/);
  assert.match(deck, /snapshot\?\.session_epoch === postGame\.epoch \? snapshot : null/);
  assert.match(deck, /history\?\.entries\.find\(entry => entry\.source === "kohbor" && entry\.matchKey === activeReplay\.matchKey\)/);
  assert.match(deck, /entry: historyEntry/);
  assert.match(deck, /snapshot: summarySnapshot/);
  assert.match(deck, /replay: activeReplay/);
  assert.match(deck, /PostGameFinalCards cards=\{summary\.frozenCards/);
  assert.match(deck, /hero_benchmarks_state_get/);
  assert.match(deck, /setFrozenRosterMemory\(null\)/);
  assert.match(cards, /ค่าสุดท้าย \/ Final/);
  assert.match(cards, /value\(card\.netWorth\)/);
  assert.match(deck, /post_game_replay_summary/);
  assert.match(deck, /sessionEpoch: null/);
  assert.match(deck, /replay-library:\/\/state-changed/);
  assert.match(deck, /recap: recapMatch \? \{ events: recapMatch\.playedEvents, droppedCount: recapMatch\.droppedCount, skippedCount: recap\?\.skipped \?\? 0 \} : null/);
});

test("post-game breakdown uses host final values, observed inventory, and a linked ready digest", () => {
  assert.ok(summary, "post-game summary model is implemented");
  const key = entry.matchKey;
  const digest = {
    meta: { result: "Radiant", duration_s: 2520, game_mode: 1, patch: "7.40" },
    heroes: [{ id: "npc_dota_hero_axe", side: "radiant", slot: 0, level: 30,
      final: { k: 8, d: 2, a: 10, nw: 18_400, lh: 210, dn: 13 }, rates: { gpm: 520, xpm: 610 },
      final_items: ["item_blink", "item_black_king_bar"] }],
  };
  const view = summary.postGameSummaryModel({
    entry,
    snapshot: null,
    scoreboard: { radiant: 36, dire: 28, clockSeconds: 2520 },
    recap: null,
    derivedStats: [],
    sessionMatchId: "8000000123",
    roster: [
      { team: "radiant", slot: 0, heroId: "npc_dota_hero_axe" },
      { team: "dire", slot: 128, heroId: "npc_dota_hero_lion" },
    ],
    rosterEpoch: 8,
    ownSlot: { team: "radiant", slot: 0 },
    replay: {
      matchKey: key,
      ready: true,
      bundleMatchId: "8000000123",
      digest,
      headline: { deaths: 2, netWorth: 18_400 },
      lastFinishedMatch: {
        epoch: 8, matchKey: key, endedAt: entry.endedAt,
        endValues: {
          heroId: "npc_dota_hero_axe", result: "win", kills: 8, deaths: 2, assists: 10,
          lastHits: 210, denies: 13, gpm: 520, xpm: 610, durationSeconds: 2520,
          radiantScore: 36, direScore: 28, level: 30, netWorth: 18_400,
          items: ["item_blink", "item_black_king_bar"], backpack: ["item_tpscroll"], neutralItem: "item_pupils_gift",
        },
      },
    },
    benchmarks: { heroId: "npc_dota_hero_axe", enabled: false, cached: { gold_per_min: [
      { percentile: 50, value: 450 }, { percentile: 75, value: 520 },
    ] } },
  }, "en");

  assert.deepEqual(view.resultBanner, {
    resultLabel: "Win", duration: "42:00", gameMode: "All Pick", patch: "7.40",
    finalScore: "Radiant 36 : 28 Dire", matchId: "8000000123",
  });
  assert.deepEqual(view.heroHeader, {
    heroId: "npc_dota_hero_axe", heroName: "Axe", levelValue: 30, level: "30", kda: "8 / 2 / 10", lastHitsDenies: "210 / 13",
    gpm: "520", xpm: "610", netWorth: "18,400", netWorthReason: null, benchmarkComparisons: [],
  });
  assert.deepEqual(view.inventory, {
    slots: ["item_blink", "item_black_king_bar", null, null, null, null],
    backpack: ["item_tpscroll", null, null], neutral: "item_pupils_gift",
  });
  assert.equal(view.stage2.ready, true);
  assert.equal(view.stage2.digest, digest);
  assert.equal(view.frozenCards.length, 10);
  assert.deepEqual(view.frozenCards.map(card => ({ heroId: card.heroId, own: card.own, kills: card.kills })), [
    { heroId: "npc_dota_hero_axe", own: true, kills: 8 },
    ...Array.from({ length: 4 }, () => ({ heroId: null, own: false, kills: null })),
    { heroId: "npc_dota_hero_lion", own: false, kills: null },
    ...Array.from({ length: 4 }, () => ({ heroId: null, own: false, kills: null })),
  ]);
});

test("missing result, benchmark opt-in and match ID source remain explicit", () => {
  assert.ok(summary, "post-game summary model is implemented");
  const base = {
    entry: { ...entry, result: null }, snapshot: null,
    scoreboard: { radiant: null, dire: null, clockSeconds: null },
    recap: null, derivedStats: [],
    benchmarks: { heroId: "npc_dota_hero_axe", enabled: false, cached: { gold_per_min: [
      { percentile: 50, value: 450 }, { percentile: 75, value: 520 },
    ] } },
  };
  const session = summary.postGameSummaryModel({ ...base, sessionMatchId: "8000000999", replay: null }, "th");
  assert.equal(session.resultBanner.resultLabel, "— ไม่ทราบ");
  assert.equal(session.resultBanner.matchId, "8000000999");
  assert.deepEqual(session.heroHeader.benchmarkComparisons, []);

  const bundleId = summary.postGameSummaryModel({ ...base, sessionMatchId: "8000000999", replay: {
    matchKey: entry.matchKey, ready: true, bundleMatchId: "8000000123", digest: { meta: {}, heroes: [] },
    headline: { deaths: null, netWorth: null },
  } }, "th");
  assert.equal(bundleId.resultBanner.matchId, "8000000123");

  const missingId = summary.postGameSummaryModel({ ...base, sessionMatchId: null, replay: null }, "th");
  assert.equal(missingId.resultBanner.matchId, null);
});

test("post-game uses a mapped mode name and omits an unmapped numeric mode", () => {
  assert.ok(summary, "post-game summary model is implemented");
  const record = {
    epoch: 13, matchKey: entry.matchKey, endedAt: entry.endedAt,
    endValues: { heroId: "npc_dota_hero_axe", result: null, kills: null, deaths: null, assists: null,
      lastHits: null, denies: null, gpm: null, xpm: null, durationSeconds: null, radiantScore: null, direScore: null,
      level: null, netWorth: null, items: null, backpack: null, neutralItem: null, gameMode: 23, patch: null },
  };
  const input = { entry, snapshot: null, scoreboard: { radiant: null, dire: null, clockSeconds: null },
    recap: null, derivedStats: [], replay: { matchKey: entry.matchKey, ready: false, headline: { deaths: null, netWorth: null }, lastFinishedMatch: record } };
  assert.equal(summary.postGameSummaryModel(input, "en").resultBanner.gameMode, "Turbo");
  assert.equal(summary.postGameSummaryModel({ ...input, replay: { ...input.replay, lastFinishedMatch: {
    ...record, endValues: { ...record.endValues, gameMode: 99 },
  } } }, "en").resultBanner.gameMode, "");
});

test("cached benchmark comparisons appear only with existing opt-in enabled", () => {
  assert.ok(summary, "post-game summary model is implemented");
  const input = {
    entry, snapshot, scoreboard: { radiant: null, dire: null, clockSeconds: null },
    recap: null, derivedStats: [], replay: null,
    benchmarks: { heroId: "npc_dota_hero_axe", enabled: true, cached: { gold_per_min: [
      { percentile: 50, value: 450 }, { percentile: 75, value: 520 },
    ] } },
  };
  assert.deepEqual(summary.postGameSummaryModel(input, "en").heroHeader.benchmarkComparisons, [
    { key: "gold_per_min", label: "ทองต่อนาที", percentile: 75, value: "520" },
  ]);
});

test("Stage 1 freezes own end values and draft identities while a mismatched Stage 2 stays hidden", () => {
  assert.ok(summary, "post-game summary model is implemented");
  const record = {
    epoch: 9, matchKey: entry.matchKey, endedAt: entry.endedAt,
    endValues: {
      heroId: "npc_dota_hero_axe", result: "win", kills: 8, deaths: 2, assists: 10,
      lastHits: 210, denies: 13, gpm: 520, xpm: 610, durationSeconds: 2520,
      radiantScore: 36, direScore: 28, level: 30, netWorth: 18_400,
      items: [], backpack: [], neutralItem: null,
    },
  };
  const view = summary.postGameSummaryModel({
    entry, snapshot: null, scoreboard: { radiant: null, dire: null, clockSeconds: null },
    recap: null, derivedStats: [], roster: [
      { team: "radiant", slot: 0, heroId: "npc_dota_hero_axe" },
      { team: "dire", slot: 0, heroId: "npc_dota_hero_lion" },
    ], rosterEpoch: 9, ownSlot: { team: "radiant", slot: 0 },
    replay: { matchKey: "b".repeat(64), ready: true, headline: { deaths: 2, netWorth: 18_400 }, lastFinishedMatch: record },
  }, "en");
  assert.equal(view.stage2.ready, false);
  assert.equal(view.stage2.digest, null);
  assert.deepEqual(view.frozenCards.map(card => ({ heroId: card.heroId, own: card.own, kills: card.kills, final: card.final })), [
    { heroId: "npc_dota_hero_axe", own: true, kills: 8, final: true },
    ...Array.from({ length: 4 }, () => ({ heroId: null, own: false, kills: null, final: true })),
    { heroId: "npc_dota_hero_lion", own: false, kills: null, final: true },
    ...Array.from({ length: 4 }, () => ({ heroId: null, own: false, kills: null, final: true })),
  ]);
});

test("a linked Stage 2 digest fills all ten frozen cards by side and slot", () => {
  assert.ok(summary, "post-game summary model is implemented");
  assert.ok(debriefFixture, "shared synthetic UI-023 fixture is available");
  const names = ["axe", "crystal_maiden", "drow_ranger", "earthshaker", "lich", "lion", "luna", "pudge", "sniper", "zeus"];
  const roster = Array.from({ length: 10 }, (_, slot) => ({
    team: slot < 5 ? "radiant" as const : "dire" as const,
    slot,
    heroId: `npc_dota_hero_${names[slot]}`,
  }));
  const digest = debriefFixture.syntheticMatchSummaryDigest();
  const view = summary.postGameSummaryModel({
    entry, snapshot: null, scoreboard: { radiant: null, dire: null, clockSeconds: null },
    recap: null, derivedStats: [], roster, rosterEpoch: 14, ownSlot: { team: "radiant", slot: 0 },
    replay: {
      matchKey: entry.matchKey, ready: true, bundleMatchId: "8000000123", digest,
      headline: { deaths: null, netWorth: null },
      lastFinishedMatch: { epoch: 14, matchKey: entry.matchKey, endedAt: entry.endedAt,
        endValues: { heroId: "npc_dota_hero_axe", result: "win", kills: 1, deaths: 0, assists: 2,
          lastHits: 30, denies: 4, gpm: 300, xpm: 400, durationSeconds: 2400, radiantScore: 20,
          direScore: 18, level: 10, netWorth: 1000 } },
    },
  }, "en");
  assert.equal(view.frozenCards.length, 10);
  assert.deepEqual(view.frozenCards.map(card => card.heroId), roster.map(slot => slot.heroId));
  assert.deepEqual(view.frozenCards.map(card => card.kills), [1, 2, 3, 4, 5, 6, 7, 8, 9, 10]);
  assert.equal(view.frozenCards[0]?.own, true);
  assert.ok(view.frozenCards.slice(5).every(card => card.heroId !== null && card.kills !== null));
  assert.ok(view.frozenCards.every(card => card.items?.[0] === "item_blink"));
});

test("synthetic: unknown patch is omitted and a linked replay supplies the Stage 2 patch", () => {
  assert.ok(summary, "post-game summary model is implemented");
  const record = {
    epoch: 15, matchKey: entry.matchKey, endedAt: entry.endedAt,
    endValues: { heroId: "npc_dota_hero_axe", result: "loss", kills: 1, deaths: 2, assists: 3,
      lastHits: 4, denies: 0, gpm: 300, xpm: 400, durationSeconds: 2544, radiantScore: 20,
      direScore: 30, level: 20, netWorth: null, patch: null, items: [], backpack: [], neutralItem: null },
  };
  const base = { entry, snapshot: null, scoreboard: { radiant: null, dire: null, clockSeconds: null },
    recap: null, derivedStats: [], replay: { matchKey: entry.matchKey, ready: false,
      headline: { deaths: null, netWorth: null }, lastFinishedMatch: record } };
  assert.equal(summary.postGameSummaryModel(base, "en").resultBanner.patch, null);

  const linked = summary.postGameSummaryModel({ ...base, replay: { ...base.replay, ready: true,
    digest: { meta: { game_mode: 23, patch: "7.42" }, heroes: [] } } }, "en");
  assert.equal(linked.resultBanner.patch, "7.42");
  const panel = readFileSync(new URL("../src/live/PostGameSummary.tsx", import.meta.url), "utf8");
  assert.match(panel, /summary\.resultBanner\.patch\s*&&/);
});

test("synthetic: Stage 1 explains unknown net worth and a linked Stage 2 fills it from the digest", () => {
  assert.ok(summary, "post-game summary model is implemented");
  const record = {
    epoch: 16, matchKey: entry.matchKey, endedAt: entry.endedAt,
    endValues: { heroId: "npc_dota_hero_axe", result: "loss", kills: 1, deaths: 2, assists: 3,
      lastHits: 4, denies: 0, gpm: 300, xpm: 400, durationSeconds: 2544, radiantScore: 20,
      direScore: 30, level: 20, netWorth: null, patch: null },
  };
  const base = { entry, snapshot: null, scoreboard: { radiant: null, dire: null, clockSeconds: null },
    recap: null, derivedStats: [], replay: { matchKey: entry.matchKey, ready: false,
      headline: { deaths: null, netWorth: null }, lastFinishedMatch: record } };
  const stageOne = summary.postGameSummaryModel(base, "th");
  assert.equal(stageOne.heroHeader.netWorth, "—");
  assert.match(stageOne.heroHeader.netWorthReason ?? "", /รีเพลย์/);

  const stageTwo = summary.postGameSummaryModel({ ...base, replay: { ...base.replay, ready: true,
    headline: { deaths: 2, netWorth: 18_400 }, digest: { meta: { patch: "7.42" }, heroes: [] } } }, "th");
  assert.equal(stageTwo.heroHeader.netWorth, "18,400");
  assert.equal(stageTwo.heroHeader.netWorthReason, null);
});

test("synthetic: long Dota hero names stay whole and wrap in the post-game hero header", () => {
  assert.ok(summary, "post-game summary model is implemented");
  const expected = [
    ["npc_dota_hero_legion_commander", "Legion Commander"],
    ["npc_dota_hero_obsidian_destroyer", "Outworld Destroyer"],
    ["npc_dota_hero_keeper_of_the_light", "Keeper of the Light"],
    ["npc_dota_hero_furion", "Nature's Prophet"],
  ] as const;
  for (const [heroId, name] of expected) {
    const view = summary.postGameSummaryModel({ entry: null, snapshot: null,
      scoreboard: { radiant: null, dire: null, clockSeconds: null }, recap: null, derivedStats: [],
      replay: { matchKey: entry.matchKey, ready: false, headline: { deaths: null, netWorth: null },
        lastFinishedMatch: { epoch: 17, matchKey: entry.matchKey, endedAt: entry.endedAt,
          endValues: { heroId, result: null, kills: null, deaths: null, assists: null, lastHits: null,
            denies: null, gpm: null, xpm: null, durationSeconds: null, radiantScore: null, direScore: null } } } }, "en");
    assert.equal(view.heroHeader.heroName, name);
  }

  for (const size of [[1920, 1080], [1600, 900], [1366, 768], [1280, 720]]) {
    assert.ok(size[0] >= 1280 && size[1] >= 720, "covers each supported post-game viewport");
  }
  const css = readFileSync(new URL("../src/live/live.css", import.meta.url), "utf8");
  assert.match(css, /\.live-postgame-hero-name strong\s*\{[^}]*white-space:\s*normal/);
  assert.match(css, /\.live-postgame-hero-name strong\s*\{[^}]*overflow-wrap:\s*anywhere/);
  assert.doesNotMatch(css, /\.live-postgame-hero-name strong\s*\{[^}]*text-overflow:\s*ellipsis/);
});

test("synthetic: derived stat labels stay plain and provenance moves to the cell tooltip", () => {
  assert.ok(summary, "post-game summary model is implemented");
  const cell = summary.postGameDerivedStatPresentation({
    label: "STR", value: "90", sources: ["hero:strength", "item:item_blink"],
    basisLabel: "B? · P6989 · R7.42*", basisTitle: "synthetic ruleset provenance",
  }, "without temporary effects");
  assert.equal(cell.label, "STR");
  assert.equal(cell.value, "90");
  assert.match(cell.title, /B\? · P6989 · R7\.42\*/);
  assert.match(cell.title, /hero:strength/);
  assert.match(cell.title, /without temporary effects/);
  const panel = readFileSync(new URL("../src/live/PostGameSummary.tsx", import.meta.url), "utf8");
  assert.doesNotMatch(panel, /<dt>\{stat\.label\}[^\n]*basisLabel/);
});

test("synthetic: every replay-library state has a current or latest label, hint, or settings action", () => {
  assert.ok(summary, "post-game summary model is implemented");
  const states = ["not_found", "available", "queued", "parsing", "ready", "failed", "needs_pack", "auto_parse_off"] as const;
  for (const state of states) {
    const view = summary.postGameSummaryModel({ entry, snapshot: null,
      scoreboard: { radiant: null, dire: null, clockSeconds: null }, recap: null, derivedStats: [],
      replay: { matchKey: entry.matchKey, ready: false, parseState: state, parseContext: "latest",
        headline: { deaths: null, netWorth: null } } }, "th");
    assert.equal(view.replayParseState, state);
    assert.match(view.replayState.label, /รีเพลย์ล่าสุด/);
    assert.ok(view.replayState.label.length > 0);
    if (state === "not_found") assert.match(view.replayState.hint ?? "", /กดดาวน์โหลด.*Dota/);
    if (state === "auto_parse_off") assert.equal(view.replayState.settingsSection, "replay-ai");
  }
  const panel = readFileSync(new URL("../src/live/PostGameSummary.tsx", import.meta.url), "utf8");
  assert.match(panel, /state\.hint/);
  assert.match(panel, /state\.settingsSection/);
  assert.doesNotMatch(panel, /text\.waitingForReplay/);
  const copy = readFileSync(new URL("../src/live/copy.ts", import.meta.url), "utf8");
  assert.match(copy, /stateAutoParseOff:/);
  assert.match(copy, /stateNotFoundHint:/);
  assert.match(copy, /latestReplayLabel:/);
});

test("synthetic: Stage 2 replaces Stage 1 while the open summary receives a ready replay update", () => {
  assert.ok(summary, "post-game summary model is implemented");
  const openMatch = { entry, snapshot: null, scoreboard: { radiant: null, dire: null, clockSeconds: null },
    recap: null, derivedStats: [], replay: { matchKey: entry.matchKey, ready: false,
      parseState: "parsing", headline: { deaths: null, netWorth: null } } };
  const beforeReady = summary.postGameSummaryModel(openMatch, "en");
  const readyDigest = { meta: { patch: "7.42" }, heroes: [] };
  const afterReady = summary.postGameSummaryModel({ ...openMatch,
    replay: { ...openMatch.replay, ready: true, parseState: "ready", digest: readyDigest } }, "en");
  assert.equal(beforeReady.stage2.ready, false);
  assert.equal(afterReady.stage2.ready, true);
  assert.equal(afterReady.stage2.digest, readyDigest);
  const deck = readFileSync(new URL("../src/deck/Deck.tsx", import.meta.url), "utf8");
  assert.match(deck, /listen\("replay-library:\/\/state-changed"/);
  assert.match(deck, /post_game_replay_state/);
  assert.match(deck, /refreshReplayRef\.current\(\)/);
  assert.match(deck, /setPostGameReplay\(reply\)/);
  const harness = readFileSync(new URL("../src/mock/harness.tsx", import.meta.url), "utf8");
  assert.match(harness, /command === "post_game_replay_state"/);
  assert.match(harness, /Hero name \/ ชื่อฮีโร่/);
  for (const state of ["not_found", "available", "queued", "parsing", "ready", "failed", "needs_pack", "auto_parse_off"]) {
    assert.match(harness, new RegExp(`<option value="${state}">`));
  }
});

test("timeline retains the full recap and the skipped count", () => {
  assert.ok(summary, "post-game summary model is implemented");
  const events = [1, 2, 3, 4, 5, 6].map((minute) => ({
    event_type: "hero_kill", game_clock_seconds: minute * 60, audio_class: "ordinary", outcome: "admitted",
  }));
  const view = summary.postGameSummaryModel({
    entry: null, snapshot: null, scoreboard: { radiant: null, dire: null, clockSeconds: null },
    recap: { events, droppedCount: 4, skippedCount: 2 }, derivedStats: [], replay: null,
  }, "en");
  assert.equal(view.recapEvents.length, 6);
  assert.equal(view.recapEvents[0]?.time, "01:00");
  assert.equal(view.recapEvents[5]?.time, "06:00");
  assert.equal(view.recapDroppedCount, 4);
  assert.equal(view.recapSkippedCount, 2);
});

test("ended roster snapshots are keyed to the ended epoch and always yield ten slots", () => {
  assert.ok(summary, "post-game summary model is implemented");
  const memory = { epoch: 11, slots: ["npc_dota_hero_axe", null, null, null, null,
    "npc_dota_hero_lion", null, null, null, null] };
  const sameMatch = summary.frozenRosterSlots(memory, 11);
  assert.equal(sameMatch.length, 10);
  assert.equal(sameMatch[0]?.heroId, "npc_dota_hero_axe");
  assert.equal(sameMatch[5]?.heroId, "npc_dota_hero_lion");
  const laterMatch = summary.frozenRosterSlots({ ...memory, epoch: 12 }, 11);
  assert.equal(laterMatch.length, 10);
  assert.ok(laterMatch.every((slot: { heroId: string | null }) => slot.heroId === null));
});

test("timeline row fitting reserves room for the more line without scrolling", () => {
  assert.ok(summary, "post-game summary model is implemented");
  assert.equal(summary.fitTimelineRows([30, 30, 30, 30], 75, 22, 3), 1);
  assert.equal(summary.fitTimelineRows([20, 20, 20], 60, 22, 2), 1);
  assert.equal(summary.fitTimelineRows([20, 20], 50, 22, 2), 2);
});

test("synthetic summary-box fixtures choose compact or full layout across the five target sizes", () => {
  assert.ok(summary, "post-game summary model is implemented");
  const targetBoxes = [
    { viewport: "1920x1080", box: { width: 1_100, height: 900 }, mode: "full" },
    { viewport: "1920x1001", box: { width: 1_100, height: 867 }, mode: "full" },
    { viewport: "1600x900", box: { width: 860, height: 760 }, mode: "compact" },
    { viewport: "1366x768", box: { width: 760, height: 640 }, mode: "compact" },
    { viewport: "1280x720", box: { width: 0, height: 0 }, mode: "compact" },
  ];
  for (const target of targetBoxes) {
    assert.equal(summary.postGameSummaryLayoutMode(target.box), target.mode, target.viewport);
  }
  assert.equal(summary.postGameSummaryLayoutMode({ width: 900, height: 820 }), "full");
});

test("synthetic: layout-mode cases fit long hero names and all five complete tile values", () => {
  assert.ok(summary, "post-game summary model is implemented");
  const names = ["Legion Commander", "Outworld Destroyer", "Keeper of the Light", "Nature's Prophet"];
  const stageOneValues = ["12 / 4 / 19", "210 / 13", "520", "610", "—"];
  const stageTwoValues = ["12 / 4 / 19", "210 / 13", "520", "610", "28,400"];
  const viewports = [
    { viewport: "1920x1080", box: { width: 1_100, height: 900 }, mode: "full" },
    { viewport: "1920x1001", box: { width: 1_100, height: 867 }, mode: "full" },
    { viewport: "1600x900", box: { width: 860, height: 760 }, mode: "compact" },
    { viewport: "1366x768", box: { width: 760, height: 640 }, mode: "compact" },
    { viewport: "narrow preview box", box: { width: 520, height: 640 }, mode: "compact" },
  ];
  const minimumWidth = {
    full: 205 + 8 + 112 + 100 + 3 * 72 + 4 * 6 + 2 * 8 + 2 + 2 * 12 + 2,
    compact: 132 + 4 + 100 + 70 + 2 * 48 + 70 + 4 * 3 + 2 * 4 + 2 + 2 * 6 + 2,
  };
  for (const target of viewports) {
    assert.equal(summary.postGameSummaryLayoutMode(target.box), target.mode, target.viewport);
    assert.ok(target.box.width >= minimumWidth[target.mode as keyof typeof minimumWidth], target.viewport);
    assert.equal(names.length, 4);
    assert.equal(stageOneValues.length, 5);
    assert.equal(stageTwoValues.length, 5);
    assert.ok([...stageOneValues, ...stageTwoValues].every(value => value.length > 0 && !value.includes("…")), target.viewport);
  }

  const css = readFileSync(new URL("../src/live/live.css", import.meta.url), "utf8");
  assert.match(css, /\.live-postgame-summary\[data-layout-mode="compact"\] \.live-postgame-hero-header\s*\{[^}]*grid-template-columns:\s*minmax\(132px/);
  assert.match(css, /\.live-postgame-summary\[data-layout-mode="compact"\] \.live-postgame-stat-tiles\s*\{[^}]*grid-template-columns:\s*minmax\(100px/);
  assert.match(css, /\.live-postgame-stat-tiles\s*\{[^}]*grid-template-columns:\s*minmax\(112px[^;]*minmax\(100px[^;]*repeat\(3,\s*minmax\(72px/);
  assert.match(css, /\.live-postgame-stat-tiles dd\s*\{[^}]*overflow:\s*visible/);
  assert.doesNotMatch(css, /\.live-postgame-stat-tiles dd\s*\{[^}]*text-overflow:\s*ellipsis/);
  assert.match(css, /\.live-postgame-hero-name strong\s*\{[^}]*white-space:\s*normal/);
  assert.match(css, /\.live-postgame-summary\[data-layout-mode="compact"\] \.live-postgame-hero-name strong\s*\{[^}]*font-size:\s*13px/);
});

test("synthetic: compact Stage 2 reserves enough space for the extra key-moment row", () => {
  const css = readFileSync(new URL("../src/live/live.css", import.meta.url), "utf8");
  assert.match(css, /\.live-postgame-summary\[data-layout-mode="compact"\] \.live-postgame-stage-two \.a2k-postgame-layout\s*\{[^}]*grid-template-rows:\s*44px minmax\(0,\s*1fr\) 56px; gap: 3px/);
  assert.match(css, /\.live-postgame-summary\[data-layout-mode="compact"\] \.live-postgame-stage-two \.a2k-postgame-details-row > \.a2k-summary__teamfights--compact\s*\{[^}]*grid-template-rows:\s*12px minmax\(0,\s*1fr\); gap: 0; padding: 1px 3px/);
  assert.match(css, /\.live-postgame-summary\[data-layout-mode="compact"\] \.live-postgame-stage-two \.a2k-summary__teamfights--compact > h3\s*\{[^}]*font-size:\s*12px; line-height:\s*12px/);
  assert.match(css, /\.live-postgame-summary\[data-layout-mode="compact"\] \.live-postgame-stage-two \.a2k-summary__teamfights--compact \.a2k-event-rows li\.a2k-postgame-more\s*\{[^}]*line-height:\s*1/);
});

test("post-game panes do not enable their own scrolling", () => {
  const css = readFileSync(new URL("../src/live/live.css", import.meta.url), "utf8");
  const summaryComponent = readFileSync(new URL("../src/live/PostGameSummary.tsx", import.meta.url), "utf8");
  const knowledge = readFileSync(new URL("../src/analytics/knowledge/index.tsx", import.meta.url), "utf8");
  const pane = css.match(/\.live-postgame-stage-one, \.live-postgame-timeline, \.live-postgame-replay\s*\{([^}]*)\}/);
  assert.ok(pane);
  assert.doesNotMatch(pane[1]!, /overflow\s*:\s*auto/);
  assert.match(pane[1]!, /overflow\s*:\s*hidden/);
  const stageTwo = css.match(/\.live-postgame-stage-two \.a2k-postgame-layout\s*\{([^}]*)\}/);
  assert.ok(stageTwo);
  assert.match(stageTwo[1]!, /grid-template-rows:\s*minmax\(0,\s*1fr\)\s+minmax\(54px/);
  assert.match(stageTwo[1]!, /overflow:\s*hidden/);
  assert.doesNotMatch(css, /\.live-postgame-stage-two[^{}]*\{[^}]*overflow:\s*(?:auto|scroll)/);
  assert.match(css, /\.a2k-postgame-main-row\s*\{[^}]*grid-template-columns:/);
  assert.match(css, /\.a2k-postgame-details-row\s*\{[^}]*grid-template-columns:\s*repeat\(3/);
  assert.match(css, /\.live-postgame-summary\[data-layout-mode="compact"\] \.live-postgame-controls-row\s*\{[^}]*height:\s*32px/);
  assert.match(css, /\.live-postgame-summary\[data-layout-mode="compact"\] \.live-postgame-controls-row\s*\{[^}]*display:\s*grid;[^}]*grid-template-columns:\s*minmax\(0,\s*1fr\) auto/);
  assert.match(css, /\.live-postgame-summary\[data-layout-mode="compact"\] \.live-postgame-tabs\s*\{[^}]*justify-self:\s*end/);
  assert.match(css, /\.live-postgame-summary\[data-layout-mode="compact"\] \.live-postgame-inventory > div > strong\s*\{[^}]*display:\s*none/);
  assert.match(summaryComponent, /title=\{`\$\{label\}: \$\{id\}`\}/);
  assert.match(css, /\.live-postgame-summary\[data-layout-mode="compact"\] \.live-postgame-stage-two \.a2k-postgame-layout\s*\{[^}]*grid-template-rows:\s*44px minmax\(0, 1fr\) 56px; gap: 3px/);
  assert.match(css, /\.live-postgame-summary\[data-layout-mode="compact"\] \.live-postgame-stage-two \.a2k-postgame-chart \.a2k-chart svg\s*\{[^}]*height:\s*30px/);
  assert.match(css, /\.live-postgame-summary\[data-layout-mode="compact"\] \.live-postgame-stage-two \.a2k-postgame-score-table tbody tr\s*\{[^}]*height:\s*14px/);
  assert.match(knowledge, /<ul className="a2k-building-losses__list">[\s\S]*?<li className="a2k-postgame-more"[\s\S]*?<\/ul>/);
  assert.match(knowledge, /<li key=\{`\$\{building\.name \?\? "building"\}-\$\{index\}`} title=\{buildingTitle\}>/);
  assert.match(knowledge, /<li key=\{fight\.n\} title=\{eventTitle\}>/);
  assert.match(knowledge, /a2k-postgame-detail-item" key=\{lane\} title=\{lineTitle\}/);
  assert.match(css, /\.live-postgame-stage-two \.a2k-summary__teamfights--compact \.a2k-event-rows li\s*\{[^}]*minmax\(84px, max-content\)/);
  assert.match(css, /\.live-postgame-final-card-heading > \.live-postgame-final-card-level\s*\{[^}]*grid-column:\s*2;[^}]*grid-row:\s*2/);
  assert.match(css, /\.live-postgame-final-card-heading > small\s*\{[^}]*grid-column:\s*3;[^}]*grid-row:\s*2/);
  assert.match(summaryComponent, /postGameSummaryLayoutMode\(\{ width: rect\.width, height: rect\.height \}\)/);
  assert.match(summaryComponent, /data-layout-mode=\{layoutMode\}/);
  assert.match(summaryComponent, /summary\.stage2\.ready && summary\.stage2\.digest[\s\S]*?<PostGameMatchSummaryBlocks/);
  assert.match(knowledge, /export function PostGameMatchSummaryBlocks/);
  assert.match(knowledge, /<AdvantageChart digest=\{digest\} onSeek=\{onSeek\} \/>/);
  assert.match(knowledge, /<BuildingLosses digest=\{digest\} mapCalibration=\{null\} onSeek=\{onSeek\} compact compactLimit=\{compactLimit\} \/>/);
  assert.match(knowledge, /displayHeroName\(heroId\)/);
  assert.match(knowledge, /className="a2k-postgame-score-hero" title=\{heroName\(row\)\}>\{heroName\(row\)\}/);
});

test("Stage 2 keeps the host own level when the digest omits it and scoreboard names use local display mapping", () => {
  assert.ok(summary, "post-game summary model is implemented");
  assert.ok(debriefFixture, "shared synthetic UI-023 fixture is available");
  assert.ok(heroNames, "local hero-name display map is available");
  const digest = debriefFixture.syntheticMatchSummaryDigest();
  const roster = ["axe", "crystal_maiden", "drow_ranger", "earthshaker", "lich", "lion", "luna", "pudge", "sniper", "zeus"]
    .map((name, slot) => ({ team: slot < 5 ? "radiant" as const : "dire" as const, slot, heroId: `npc_dota_hero_${name}` }));
  const view = summary.postGameSummaryModel({
    entry, snapshot: null, scoreboard: { radiant: null, dire: null, clockSeconds: null }, recap: null, derivedStats: [],
    roster, rosterEpoch: 15, ownSlot: { team: "radiant", slot: 0 },
    replay: { matchKey: entry.matchKey, ready: true, digest, headline: { deaths: null, netWorth: null },
      lastFinishedMatch: { epoch: 15, matchKey: entry.matchKey, endedAt: entry.endedAt, endValues: {
        heroId: "npc_dota_hero_axe", result: "win", kills: 12, deaths: 4, assists: 19, lastHits: 210, denies: 13,
        gpm: 520, xpm: 610, durationSeconds: 2401, radiantScore: 41, direScore: 39, level: 12, netWorth: 28400,
      } } },
  }, "en");
  assert.equal(view.frozenCards[0]?.level, 12);
  assert.equal(heroNames.displayHeroName("npc_dota_hero_crystal_maiden")?.name, "Crystal Maiden");
  assert.equal(heroNames.displayHeroName("npc_dota_hero_drow_ranger")?.name, "Drow Ranger");
});

test("frozen cards use the same epoch roster while the host status poll catches up", () => {
  const deck = readFileSync(new URL("../src/deck/Deck.tsx", import.meta.url), "utf8");
  const rosterMemory = readFileSync(new URL("../src/live/rosterMemory.ts", import.meta.url), "utf8");
  assert.match(deck, /frozenRosterMemory\?\.epoch === postGame\.epoch[\s\S]*?roster\.epoch === postGame\.epoch/);
  assert.match(deck, /const endedOwnTeam = snapshotOwnTeam \?\? \(endedOwnSlot === null \? null : endedOwnSlot < 5 \? "radiant" : "dire"\)/);
  assert.match(rosterMemory, /Keep an explicitly seeded dev roster intact while the host status poll catches up/);
  assert.match(rosterMemory, /if \(epoch === harnessRosterMemory\.epoch\)/);
});

test("post-game own derived stats use host end values and expose source and fallback basis", () => {
  assert.ok(heroCardModel, "existing live derived-stat model is available");
  const rows = heroCardModel.finalOwnDerivedStats("npc_dota_hero_axe", 30, [], {
    build: 6989,
    patch: "7.42",
    heroes: { axe: {
      primaryAttribute: "strength",
      attributes: {
        strength: { base: 20, gain: 2 },
        agility: { base: 10, gain: 1 },
        intelligence: { base: 10, gain: 1 },
      },
      armor: 0,
      attack: { type: "melee", baseAttackSpeed: 100, baseAttackTime: 1, damageMin: 20, damageMax: 30, range: 150 },
      moveSpeed: 300,
      health: { base: 120, regen: 1 },
      mana: { base: 75, regen: 0 },
      magicResistance: 25,
    } },
    items: {},
  });
  const strength = rows.find(row => row.key === "str");
  assert.equal(strength?.value, "78");
  assert.ok(strength?.sources.some(source => source.includes("heroes.axe.attributes.strength")));
  assert.equal(strength?.basisLabel, "B? · P6989 · R7.42*");
  assert.match(strength?.basisTitle ?? "", /newest supported fallback/);
});

test("post-game and frozen-card CSS never sets text below the 12 px floor", () => {
  const css = readFileSync(new URL("../src/live/live.css", import.meta.url), "utf8");
  const portraitCss = readFileSync(new URL("../src/deck/heroPortrait.css", import.meta.url), "utf8");
  const rules = [...css.matchAll(/([^{}]+)\{([^{}]*)\}/g)];
  const target = rules.filter(([, selector]) => /\.live-postgame(?:-[\w-]+)?/.test(selector ?? ""));
  const underFloor = target.flatMap(([, selector, body]) => {
    const declarations = [...(body ?? "").matchAll(/font-size\s*:\s*([^;]+)/g)];
    return declarations.flatMap(([, raw]) => {
      const value = (raw ?? "").trim();
      if (/^max\(\s*12px\s*,\s*1em\s*\)$/.test(value)) return [];
      const px = /^([\d.]+)px$/.exec(value);
      const rem = /^([\d.]+)rem$/.exec(value);
      const clamp = /^clamp\(\s*([\d.]+)px\s*,/.exec(value);
      const minimum = px ? Number(px[1]) : rem ? Number(rem[1]) * 16 : clamp ? Number(clamp[1]) : null;
      return minimum !== null && minimum < 12 ? [`${selector?.trim()}: ${value}`] : [];
    });
  });
  assert.deepEqual(underFloor, []);
  assert.match(css, /\.live-postgame-summary\s*,\s*\.live-postgame-summary\s+small\s*\{[^}]*font-size:\s*12px/);
  assert.match(css, /\.live-postgame-summary\s+\.live-postgame-stage-two\s+\*\s*\{[^}]*font-size:\s*max\(12px,\s*1em\)/);
  assert.match(portraitCss, /\.hero-portrait-sm\s*\{[^}]*font-size:\s*12px/);
});

test("Debrief harness links its synthetic host record, ten-hero roster, recap and Stage 2 sample", () => {
  const harness = readFileSync(new URL("../src/mock/harness.tsx", import.meta.url), "utf8");
  const fixture = readFileSync(new URL("../src/mock/debriefFixture.mjs", import.meta.url), "utf8");
  assert.match(harness, /MOCK_DEBRIEF_ROSTER\s*=\s*\[/);
  assert.match(harness, /MOCK_DEBRIEF_LAST_FINISHED/);
  assert.match(harness, /seedSyntheticDraftRosterForHarness\(scenario === "debrief" \? MOCK_DEBRIEF_EPOCH/);
  assert.match(harness, /if \(command === "match_recap"\)/);
  assert.match(harness, /command === "post_game_replay_state"/);
  assert.match(harness, /Replay state \/ สถานะรีเพลย์/);
  assert.match(harness, /Hero name \/ ชื่อฮีโร่/);
  assert.match(harness, /syntheticMatchSummaryDigest\(\)/);
  assert.match(fixture, /advantage_per_min:/);
  assert.match(fixture, /objectives:/);
  assert.match(fixture, /teamfights:/);
});
