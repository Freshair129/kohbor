import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { test } from "node:test";
import {
  formatReplayMatchDate,
  groupReplayMatches,
  replayEntryHero,
  replayEntryResult,
  replayEntryTimeMs,
} from "../src/replay/replayLibraryModel.ts";
import type { ReplayLibraryEntry } from "../src/replay/replayLibraryClient";

function match(match_id: string, at: number | null, result: "win" | "loss" | null = null): ReplayLibraryEntry {
  return {
    match_id, match_key: null, source: "dota_replays_folder", engine_version: null,
    parse_time_unix_ms: null, status: { state: "ready" }, bundle_bytes: 0, storage_bytes: 0,
    created_at_unix_ms: 0, match_date_utc: at === null ? null : new Date(at).toISOString(),
    own_hero: null, duration_s: 1200, patch: "7.42", bundle_hash: null,
    local_history: result === null ? null : { result, kills: null, deaths: null, assists: null, ended_at_unix_s: Math.floor(at! / 1000) },
  };
}

test("match list groups newest Bangkok day first and keeps unknown dates last", () => {
  const now = Date.UTC(2026, 10, 27, 12);
  const groups = groupReplayMatches([
    match("old", Date.UTC(2026, 10, 25, 12), "loss"),
    match("today-early", Date.UTC(2026, 10, 26, 17), "win"),
    match("unknown", null),
    match("today-late", Date.UTC(2026, 10, 26, 19), "loss"),
  ], { dateFilter: "all", resultFilter: "all", specificDay: "", now, unknownDay: "ไม่ทราบวัน" });
  assert.deepEqual(groups.map(group => group.dateKey), ["2026-11-27", "2026-11-25", null]);
  assert.deepEqual(groups[0]!.entries.map(entry => entry.match_id), ["today-late", "today-early"]);
  assert.match(groups[0]!.label, /27 พ\.ย\. 2569/);
});

test("date and result filters keep explicit unknowns honest and support a specific day", () => {
  const now = Date.UTC(2026, 10, 27, 12);
  const entries = [
    match("win", Date.UTC(2026, 10, 26, 20), "win"),
    match("loss", Date.UTC(2026, 10, 25, 20), "loss"),
    match("unknown-result", Date.UTC(2026, 10, 26, 21)),
    match("unknown-day", null),
  ];
  assert.deepEqual(groupReplayMatches(entries, { dateFilter: "today", resultFilter: "all", specificDay: "", now, unknownDay: "ไม่ทราบวัน" }).flatMap(group => group.entries).map(entry => entry.match_id), ["unknown-result", "win"]);
  assert.deepEqual(groupReplayMatches(entries, { dateFilter: "all", resultFilter: "loss", specificDay: "", now, unknownDay: "ไม่ทราบวัน" }).flatMap(group => group.entries).map(entry => entry.match_id), ["loss"]);
  assert.deepEqual(groupReplayMatches(entries, { dateFilter: "specific", resultFilter: "all", specificDay: "2026-11-27", now, unknownDay: "ไม่ทราบวัน" }).flatMap(group => group.entries).map(entry => entry.match_id), ["unknown-result", "win"]);
});

test("unlinked identity and ISO dates use local bundle identity without exposing account IDs", () => {
  const entry = match("unlinked", Date.UTC(2026, 10, 26, 18));
  entry.own_hero = "npc_dota_hero_axe";
  assert.equal(replayEntryHero(entry), null);
  entry.bundle_identity = { own_hero: "npc_dota_hero_axe", result: "win" };
  assert.equal(replayEntryHero(entry), "npc_dota_hero_axe");
  assert.equal(replayEntryResult(entry), "win");
  assert.equal(replayEntryTimeMs(entry), Date.UTC(2026, 10, 26, 18));
  assert.match(formatReplayMatchDate(replayEntryTimeMs(entry)!, "th"), /27 พ\.ย\. 2569/);
  assert.equal(JSON.stringify(entry).includes("account_id"), false);
});

test("match list can group and count 200 replay rows", () => {
  const now = Date.UTC(2026, 10, 27, 12);
  const entries = Array.from({ length: 200 }, (_, index) => match(String(index), now - index * 60_000, index % 2 ? "loss" : "win"));
  const groups = groupReplayMatches(entries, { dateFilter: "all", resultFilter: "all", specificDay: "", now, unknownDay: "ไม่ทราบวัน" });
  assert.equal(groups.reduce((sum, group) => sum + group.entries.length, 0), 200);
});

test("match list header and every row share the fixed action-column grid", () => {
  const css = readFileSync(new URL("../src/analytics/analytics.css", import.meta.url), "utf8");
  assert.match(css, /--analytics-match-main-columns:[^;]+;\s*--analytics-match-list-columns:\s*var\(--analytics-match-main-columns\)\s+248px/);
  assert.match(css, /\.analytics-match-list-columns\s*\{[^}]*grid-template-columns:\s*var\(--analytics-match-list-columns\)/s);
  assert.match(css, /\.analytics-list-overlay\s+\.analytics-match-row\s*\{[^}]*grid-template-columns:\s*var\(--analytics-match-list-columns\)/s);
  assert.match(css, /\.analytics-match-row-actions\s*\{[^}]*grid-column:\s*7;[^}]*width:\s*248px/s);
});
