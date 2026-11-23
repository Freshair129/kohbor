import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";
import {
  bestHeroPanel, countingNotes, EMPTY_HISTORY, formatDuration, heroCards, localMonthStart, matchRows, newsColumn,
  parseMatchHistory, parseNewsView, percent, placeholderRowCount, PLACEHOLDER_HEROES, statTiles, trendPolyline,
  type NewsPost,
} from "../src/overview/overviewModel.ts";
import { overviewCopy } from "../src/overview/overviewCopy.ts";

// Synthetic host replies shaped like `match_history_get` (apps/desktop/src-tauri/src/match_history.rs).
function entry(n: number, values: Record<string, unknown> = {}) {
  return {
    match_key: n.toString(16).padStart(64, "0"),
    hero: "npc_dota_hero_axe",
    result: "win",
    end: "completed",
    kills: 5, deaths: 2, assists: 9,
    duration_s: 1925,
    ended_at: 1_795_401_600 - n * 3600,
    match_type: "unknown",
    source: "kohbor",
    ...values,
  };
}

function reply(summary: Record<string, unknown>, entries: unknown[]) {
  return {
    schema_version: "1.0.0",
    summary: {
      recorded: entries.length, counted: 0, excluded_match_type: 0, excluded_no_result: 0,
      overall: { wins: 0, losses: 0, win_rate: null },
      last: { results: [], wins: 0, losses: 0, win_rate: null },
      trend: { window: 10, points: [] },
      top_heroes: [],
      ...summary,
    },
    entries,
  };
}

test("unknown-type matches with a result count, labelled all modes; abandoned stays '—'", () => {
  // Host summary under "Count all, labelled" (owner 2026-11-23): two unknown-type wins count, the
  // abandoned match does not.
  const history = parseMatchHistory(reply({
    recorded: 3, counted: 2, counted_unknown_type: 2, excluded_no_result: 1,
    overall: { wins: 2, losses: 0, win_rate: 100 },
    last: { results: ["win", "win"], wins: 2, losses: 0, win_rate: 100 },
    top_heroes: [{ hero: "npc_dota_hero_axe", games: 3, counted_games: 2, wins: 2, losses: 0, win_rate: null }],
  }, [entry(1), entry(2), entry(3, { result: null, end: "abandoned", duration_s: null })]))!;
  assert.ok(history);
  const tiles = statTiles(history, "th");
  assert.equal(tiles.overall.value, "100.0%");
  assert.equal(tiles.overall.known, true);
  assert.equal(tiles.overall.scope, "รวมทุกโหมด (ยังแยก ranked/unranked ไม่ได้)");
  assert.equal(statTiles(history, "en").overall.scope, "All modes (ranked/unranked not yet separated)");
  assert.equal(tiles.last.title, "2 แมตช์ล่าสุด");
  assert.deepEqual(tiles.last.slots.slice(0, 3), ["win", "win", null]);
  assert.equal(tiles.last.slots.length, 20);
  assert.equal(tiles.donut.total, 3);
  assert.deepEqual(tiles.donut.segments.map((x) => [x.kind, Math.round(x.length)]), [["win", 67], ["none", 33]]);
  assert.equal(tiles.trend.points, null);
  assert.match(tiles.trend.need!, /10 แมตช์/);
  const notes = countingNotes(history, "en");
  assert.equal(notes.length, 2);
  assert.match(notes[0]!, /does not report the match type through GSI/);
  assert.match(notes[1]!, /not a loss/);
  assert.equal(heroCards(history, "en")[0]!.record, "2–0");

  const rows = matchRows(history, "en", null);
  assert.equal(rows[0]!.result, "W");
  assert.equal(rows[0]!.counted, true);
  assert.match(rows[0]!.typeTitle!, /unknown/);
  assert.equal(rows[2]!.counted, false, "no result is never counted");
  assert.equal(rows[2]!.result, "—", "abandoned is never a loss");
  assert.equal(rows[2]!.resultTone, "unknown");
  assert.equal(rows[2]!.duration, "—");
  assert.equal(rows[0]!.kda, "5 / 2 / 9");
  assert.equal(rows[0]!.duration, "32:05");
  assert.equal(rows[0]!.heroName, "Axe");
});

test("counted matches: honest last-N title, squares oldest to newest, hero W-L below five games", () => {
  const history = parseMatchHistory(reply({
    recorded: 6, counted: 6,
    overall: { wins: 4, losses: 2, win_rate: 66.66666 },
    last: { results: ["win", "loss", "win", "win", "loss", "win"], wins: 4, losses: 2, win_rate: 66.66666 },
    top_heroes: [
      { hero: "npc_dota_hero_axe", games: 4, counted_games: 4, wins: 3, losses: 1, win_rate: null },
      { hero: "npc_dota_hero_lion", games: 6, counted_games: 6, wins: 2, losses: 4, win_rate: 33.333 },
    ],
  }, [entry(1, { match_type: "ranked" })]))!;
  const tiles = statTiles(history, "en");
  assert.equal(tiles.overall.value, "66.7%");
  assert.equal(tiles.overall.scope, null, "known types only: no all-modes label");
  assert.equal(tiles.overall.sub, "4–2 · 6 matches");
  assert.equal(tiles.last.title, "Last 6 matches");
  assert.deepEqual(tiles.last.slots.slice(0, 6), ["win", "loss", "win", "win", "loss", "win"].reverse());
  assert.deepEqual(tiles.last.slots.slice(6), Array(14).fill(null));
  const cards = heroCards(history, "en");
  assert.equal(cards[0]!.record, "3–1");
  assert.equal(cards[0]!.usesRate, false);
  assert.equal(cards[1]!.record, "33.3%");
  assert.equal(cards[1]!.tone, "down");
  assert.equal(matchRows(history, "en", null)[0]!.counted, true);
});

test("once types are known, bot, custom and lobby are excluded and noted", () => {
  const history = parseMatchHistory(reply({ recorded: 3, counted: 1, excluded_match_type: 2,
    overall: { wins: 1, losses: 0, win_rate: 100 } },
  [entry(1, { match_type: "ranked" }), entry(2, { match_type: "bot" }), entry(3, { match_type: "lobby" })]))!;
  const rows = matchRows(history, "en", null);
  assert.deepEqual(rows.map((r) => r.counted), [true, false, false]);
  assert.match(countingNotes(history, "en")[1]!, /2 bot, custom or lobby matches not counted/);
  assert.equal(statTiles(history, "en").overall.scope, null);
});

test("trend draws only with points; one point is a flat line", () => {
  assert.equal(trendPolyline([]), null);
  assert.equal(trendPolyline([50]), "0,32.0 300,32.0");
  assert.equal(trendPolyline([100, 0]), "0.0,4.0 300.0,60.0");
});

test("malformed host replies are unavailable, not zeros", () => {
  assert.equal(parseMatchHistory(null), null);
  assert.equal(parseMatchHistory({ schema_version: "2.0.0", summary: {}, entries: [] }), null);
  const history = parseMatchHistory(reply({}, [{ ...entry(1), source: "opendota" }, { ...entry(2), ended_at: -1 }]))!;
  assert.equal(history.entries.length, 0);
  assert.equal(percent(null), "—");
  assert.equal(formatDuration(null), "—");
});

function post(id: string, type: string, publishedAt: number, extra: Record<string, unknown> = {}) {
  return {
    id, type, published_at: publishedAt, pinned: false,
    title: { th: `หัวข้อ ${id}`, en: `Title ${id}` },
    summary: { th: "สรุป", en: "Summary" },
    body: null, image: null, links: [], app_version: null, game_patch: null,
    ...extra,
  };
}

test("news view: pinned first, type filter with counts, four posts until expanded", () => {
  const view = parseNewsView({
    state: "ok", enabled: true, disclosure_seen: false, feed_url: "https://raw.githubusercontent.com/x", fetched_at: 100,
    last_error: null,
    posts: [
      post("a", "announcement", 10, { pinned: true }),
      post("b", "app_update", 50),
      post("c", "patch_notes", 40, { links: [{ label: { th: "แพตช์", en: "Patch" }, url: "https://www.dota2.com/patches" }] }),
      post("d", "app_update", 30),
      post("e", "announcement", 20, { image: { data_url: "https://evil.invalid/x.png", width: 10, height: 10, alt: { th: "ก", en: "a" } } }),
      post("f", "poll", 60),
    ],
  })!;
  assert.ok(view);
  assert.equal(view.posts.length, 5, "unknown type dropped");
  assert.equal(view.posts.find((p) => p.id === "e")!.image!.dataUrl, null, "only data: image URLs are rendered");
  assert.deepEqual(view.posts.find((p) => p.id === "c")!.links, [{ label: { th: "แพตช์", en: "Patch" }, index: 0 }]);

  const all = newsColumn(view.posts, "all", false);
  assert.equal(all.pinned!.id, "a");
  assert.deepEqual(all.posts.map((p: NewsPost) => p.id), ["b", "c", "d"]);
  assert.equal(all.hidden, 1);
  assert.deepEqual(all.counts, { all: 5, announcement: 2, app_update: 2, patch_notes: 1 });
  const expanded = newsColumn(view.posts, "all", true);
  assert.equal(expanded.hidden, 0);
  assert.equal(expanded.posts.length, 4);
  const apps = newsColumn(view.posts, "app_update", false);
  assert.equal(apps.pinned, null);
  assert.deepEqual(apps.posts.map((p: NewsPost) => p.id), ["b", "d"]);
});

test("news states and disclosure copy are present in both languages", () => {
  for (const state of ["loading", "ok", "offline", "unavailable", "disabled"]) {
    assert.equal(parseNewsView({ state, enabled: state !== "disabled", posts: [] })!.state, state);
  }
  assert.equal(parseNewsView({ state: "weird", posts: [] }), null);
  for (const locale of ["th", "en"] as const) {
    const t = overviewCopy(locale).news;
    assert.match(t.disclosureBody, /GitHub/);
    assert.match(t.disclosureBody, /IP/);
    assert.match(t.settingsBody("https://raw.githubusercontent.com/Freshair129/kohbor-updates/main/news.json"), /news\.json/);
    assert.match(overviewCopy(locale).stats.openDotaTitle, /OpenDota/);
  }
});

test("the page keeps OpenDota disabled and opens links only by post id and index", () => {
  const page = readFileSync(new URL("../src/overview/OverviewPage.tsx", import.meta.url), "utf8");
  assert.match(page, /openDotaButton\}<\/button>/);
  assert.match(page, /disabled aria-disabled="true">\{t\.openDotaButton\}/);
  assert.match(page, /client\.openNewsLink\(post\.id, index\)/);
  assert.doesNotMatch(page, /href=/, "no renderer-side links");
  const client = readFileSync(new URL("../src/overview/overviewClient.ts", import.meta.url), "utf8");
  assert.doesNotMatch(client, /https?:\/\//, "the renderer never names a URL to fetch");
});

test("no data still lays out every box: 0.0%, 0 matches, empty ring, 20 empty slots, empty trend frame, 5 empty rows", () => {
  for (const locale of ["th", "en"] as const) {
    const tiles = statTiles(EMPTY_HISTORY, locale);
    assert.equal(tiles.overall.value, "0.0%");
    assert.equal(tiles.overall.known, false);
    assert.ok(tiles.overall.scope, "all-modes label kept");
    assert.equal(tiles.last.value, "0.0%");
    assert.equal(tiles.last.slots.length, 20);
    assert.ok(tiles.last.slots.every((x) => x === null));
    assert.equal(tiles.donut.total, 0);
    assert.deepEqual(tiles.donut.segments, []);
    assert.equal(tiles.trend.points, null);
    assert.ok(tiles.trend.need);
  }
  assert.equal(statTiles(EMPTY_HISTORY, "th").overall.sub, "0–0 · 0 แมตช์");
  assert.match(statTiles(EMPTY_HISTORY, "th").trend.need!, /ต้องมีอย่างน้อย 10 แมตช์/);
  assert.equal(placeholderRowCount(0), 5);
  assert.equal(placeholderRowCount(3), 2);
  assert.equal(placeholderRowCount(8), 0);
  // Fewer than 10 counted matches never draw a line even if a point were present.
  assert.equal(statTiles({ ...EMPTY_HISTORY, counted: 9, trend: [50] }, "en").trend.points, null);
});

test("best-hero panels: host heroes first, then labelled zero placeholders that never repeat a real hero", () => {
  const empty = bestHeroPanel([], 3, "th");
  assert.deepEqual(empty.map((c) => c.hero), [...PLACEHOLDER_HEROES]);
  for (const card of empty) {
    assert.equal(card.placeholder, true);
    assert.equal(card.rate, "0.0%");
    assert.equal(card.record, "0–0");
    assert.equal(card.kda, "0.0 / 0.0 / 0.0");
    assert.equal(card.games, "0 เกม");
    assert.equal(card.title, "ยังไม่มีข้อมูล — แสดงตัวอย่างช่อง");
  }
  assert.equal(bestHeroPanel([], 3, "en")[0]!.title, "No data yet — showing a placeholder slot");
  const real = bestHeroPanel([{
    hero: "npc_dota_hero_pudge", basis: "win_rate", games: 4, countedGames: 4, wins: 3, losses: 1, winRate: 75,
    avgKills: 7.25, avgDeaths: 3, avgAssists: 10.5,
  }], 3, "en");
  assert.deepEqual(real.map((c) => [c.hero, c.placeholder]), [
    ["npc_dota_hero_pudge", false], ["npc_dota_hero_juggernaut", true], ["npc_dota_hero_crystal_maiden", true],
  ]);
  assert.equal(real[0]!.rate, "75.0%");
  assert.equal(real[0]!.kda, "7.3 / 3.0 / 10.5");
  assert.equal(real[0]!.basis, "By win rate");
  assert.equal(real[0]!.basisTitle, "Ranked by win rate (at least 3 games)");
  const parsed = parseMatchHistory({
    schema_version: "1.0.0",
    summary: { overall: {}, last: {}, trend: {}, top_heroes: [], best_heroes: { min_games: 3, all_time: [
      { hero: "npc_dota_hero_axe", basis: "most_played", games: 1, counted_games: 0, wins: 0, losses: 0, win_rate: null,
        avg_kills: null, avg_deaths: null, avg_assists: null }], this_month: null } },
    entries: [],
  })!;
  assert.equal(parsed.best.allTime[0]!.basis, "most_played");
  assert.deepEqual(parsed.best.thisMonth, []);
  assert.equal(bestHeroPanel(parsed.best.allTime, 3, "en")[0]!.rate, "—", "a real unknown rate stays a dash");
});

test("the month start sent to the host is local midnight on the 1st", () => {
  const start = localMonthStart(new Date(2026, 10, 23, 15, 30));
  const d = new Date(start * 1000);
  assert.deepEqual([d.getFullYear(), d.getMonth(), d.getDate(), d.getHours(), d.getMinutes()], [2026, 10, 1, 0, 0]);
  assert.equal(localMonthStart(new Date(2026, 10, 1, 0, 0, 0)), start);
  assert.ok(localMonthStart(new Date(2026, 9, 31, 23, 59, 59)) < start);
});

test("the page always renders the full layout and marks placeholders", () => {
  const page = readFileSync(new URL("../src/overview/OverviewPage.tsx", import.meta.url), "utf8");
  assert.match(page, /EMPTY_HISTORY/);
  const stats = page.slice(page.indexOf("function StatsColumn("), page.indexOf("function Post("));
  assert.doesNotMatch(stats, /t\.emptyTitle|ov-empty/, "no collapsed empty state in the stats column");
  assert.match(page, /ov-ph-chip/);
  assert.match(page, /ov-row-empty/);
  const css = readFileSync(new URL("../src/overview/overview.css", import.meta.url), "utf8");
  assert.match(css, /\.ov-hcard\.placeholder \.ov-hcard-img \{ filter: grayscale\(1\)/);
});
