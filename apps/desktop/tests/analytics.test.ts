import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { test } from "node:test";
import { analyticsLayout, analyticsViewState, UNKNOWN_VALUE, replayRowFacts } from "../src/analytics/analyticsModel.ts";
import type { ReplayLibraryEntry } from "../src/replay/replayLibraryClient.ts";

const entry = (partial: Partial<ReplayLibraryEntry> = {}): ReplayLibraryEntry => ({
  match_id: "1", source: "dota_replays_folder", source_path: "", dem_sha256: null,
  engine_version: null, engine_sha256: null, parse_time_unix_ms: null,
  status: { state: "ready" }, bundle_bytes: 100, storage_bytes: 150,
  created_at_unix_ms: 1, match_date_utc: null, own_hero: null, duration_s: null, patch: null, ...partial,
});

test("Analytics view model covers the requested page states", () => {
  const base = { entries: [], selectedId: null, selectedReady: false, openDocument: false, aiAvailable: false, showList: false, showSettings: false };
  assert.equal(analyticsViewState(base), "empty");
  assert.equal(analyticsViewState({ ...base, entries: [entry()], selectedId: "1" }), "parsing");
  assert.equal(analyticsViewState({ ...base, entries: [entry()], selectedId: "1", selectedReady: true }), "noai");
  assert.equal(analyticsViewState({ ...base, entries: [entry()], selectedId: "1", selectedReady: true, aiAvailable: true }), "ready");
  assert.equal(analyticsViewState({ ...base, entries: [entry()], selectedId: "1", selectedReady: true, openDocument: true }), "aidoc");
  assert.equal(analyticsViewState({ ...base, showList: true }), "list");
  assert.equal(analyticsViewState({ ...base, showSettings: true }), "settings");
});

test("Analytics fits both reference viewports without a page scroll", () => {
  for (const [width, height] of [[1440, 900], [1920, 1080]]) {
    const layout = analyticsLayout(width, height);
    assert.equal(layout.overlap, false, `${width}x${height}`);
    assert.ok(layout.center >= 520);
    assert.equal(layout.pageScroll, false);
  }
  const collapsed = analyticsLayout(1440, 900, true, true);
  assert.ok(collapsed.center > analyticsLayout(1440, 900).center);
  assert.equal(collapsed.mapMin, 600);
  assert.equal(analyticsLayout(1440, 900).mapMin, 600);
});

test("Analytics CSS keeps a fixed three-column canvas with an internally scrolling document", () => {
  const css = readFileSync(new URL("../src/analytics/analytics.css", import.meta.url), "utf8");
  assert.match(css, /grid-template-columns:\s*minmax\(210px,[^)]*\)\s+minmax\(600px,[^)]*\)\s+minmax\(156px,[^)]*\)/);
  assert.match(css, /\.analytics-page\s*\{[^}]*overflow:\s*hidden/s);
  assert.match(css, /\.analytics-columns\s*\{[^}]*overflow:\s*hidden/s);
  assert.match(css, /\.analytics-markdown\s*\{[^}]*overflow-y:\s*auto/s);
});

test("missing replay facts remain unknown and never become zero", () => {
  const facts = replayRowFacts(entry());
  assert.equal(facts.date, UNKNOWN_VALUE);
  assert.equal(facts.ownHero, UNKNOWN_VALUE);
  assert.equal(facts.duration, UNKNOWN_VALUE);
  assert.equal(facts.sizeBytes, 100);
  assert.equal(replayRowFacts(entry({ bundle_bytes: 0 })).sizeBytes, null);
});
