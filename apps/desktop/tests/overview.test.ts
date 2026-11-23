import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";
import {
  countingNotes, formatDuration, heroCards, matchRows, newsColumn, parseMatchHistory, parseNewsView, percent,
  statTiles, trendPolyline, type NewsPost,
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

test("unknown-type matches only: every rate is '—' and the note says why", () => {
  const history = parseMatchHistory(reply({
    recorded: 3, excluded_match_type: 2, excluded_no_result: 1,
    top_heroes: [{ hero: "npc_dota_hero_axe", games: 3, counted_games: 0, wins: 0, losses: 0, win_rate: null }],
  }, [entry(1), entry(2), entry(3, { result: null, end: "abandoned", duration_s: null })]))!;
  assert.ok(history);
  const tiles = statTiles(history, "th");
  assert.equal(tiles.overall.value, "—");
  assert.equal(tiles.overall.known, false);
  assert.equal(tiles.last.value, "—");
  assert.deepEqual(tiles.last.squares, []);
  assert.equal(tiles.last.title, "20 แมตช์ล่าสุด");
  assert.equal(tiles.trend.points, null);
  assert.match(tiles.trend.need!, /10 แมตช์/);
  const notes = countingNotes(history, "en");
  assert.equal(notes.length, 3);
  assert.match(notes[1]!, /2 matches not counted: the game does not report the match type through GSI/);
  assert.match(notes[2]!, /not a loss/);
  assert.equal(heroCards(history, "en")[0]!.record, "—", "no counted game, no record");

  const rows = matchRows(history, "en", null);
  assert.equal(rows[0]!.result, "W");
  assert.equal(rows[0]!.counted, false);
  assert.match(rows[0]!.typeTitle!, /unknown/);
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
  assert.match(tiles.overall.sub, /^4–2 · from 6 counted matches/);
  assert.equal(tiles.last.title, "Last 6 matches");
  assert.deepEqual(tiles.last.squares, ["win", "loss", "win", "win", "loss", "win"].reverse());
  const cards = heroCards(history, "en");
  assert.equal(cards[0]!.record, "3–1");
  assert.equal(cards[0]!.usesRate, false);
  assert.equal(cards[1]!.record, "33.3%");
  assert.equal(cards[1]!.tone, "down");
  assert.equal(matchRows(history, "en", null)[0]!.counted, true);
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
