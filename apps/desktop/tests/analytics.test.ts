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
  for (const [width, height, expectedDocs, expectedCenter, expectedChat] of [[1440, 900, 28, 939, 289], [1920, 1080, 273, 1040, 423]] as const) {
    const layout = analyticsLayout(width, height);
    assert.equal(layout.overlap, false, `${width}x${height}`);
    assert.equal(layout.docs, expectedDocs);
    assert.equal(layout.center, expectedCenter);
    assert.equal(layout.right, expectedChat);
    assert.equal(layout.pageScroll, false);
  }
  const collapsed = analyticsLayout(1440, 900, true, true);
  assert.ok(collapsed.center > analyticsLayout(1440, 900).center);
  assert.equal(collapsed.mapMin, 300);
  assert.equal(analyticsLayout(1440, 900).mapMin, 300);
});

test("Analytics CSS reserves the fixed chat width and uses a drawer for the compact document panel", () => {
  const css = readFileSync(new URL("../src/analytics/analytics.css", import.meta.url), "utf8");
  assert.match(css, /grid-template-columns:\s*273px\s+minmax\(0,\s*1fr\)\s+423px/);
  assert.match(css, /grid-template-columns:\s*28px\s+minmax\(0,\s*1fr\)\s+289px/);
  assert.match(css, /\.analytics-page\[data-left-collapsed="false"\] \.analytics-docs\s*\{[^}]*position:\s*absolute/s);
  assert.match(css, /--hc-scale:\s*0\.88/);
  assert.match(css, /\.analytics-page\s*\{[^}]*overflow:\s*hidden/s);
  assert.match(css, /\.analytics-columns\s*\{[^}]*overflow:\s*hidden/s);
  assert.match(css, /\.analytics-markdown\s*\{[^}]*overflow-y:\s*auto/s);
});

test("Transcript view renders stored user and assistant turns through the safe Markdown renderer", () => {
  const page = readFileSync(new URL("../src/analytics/AnalyticsPage.tsx", import.meta.url), "utf8");
  assert.match(page, /analytics_chat_transcript/);
  assert.match(page, /turn\.role === "assistant"/);
  assert.match(page, /renderMarkdownStreaming\(turn\.content/);
  assert.doesNotMatch(page, /<p>\{answer\}<\/p>/);
  assert.doesNotMatch(page, /ผู้ช่วยเห็น:/);
});

test("AI submit sends only the current question, selected hero, scrubber time and selected match", () => {
  const page = readFileSync(new URL("../src/analytics/AnalyticsPage.tsx", import.meta.url), "utf8");
  const request = page.match(/invoke<string>\("analytics_chat_submit",\s*\{([^}]*)\}/)?.[1] ?? "";
  assert.match(request, /matchId:\s*selectedId/);
  assert.match(request, /question:\s*userQuestion/);
  assert.match(request, /selectedHero:\s*hero/);
  assert.match(request, /scrubberTimeS:\s*currentTime/);
  assert.doesNotMatch(request, /include_|openDocument|documentText|contextChips/i);
});

test("missing replay facts remain unknown and never become zero", () => {
  const facts = replayRowFacts(entry());
  assert.equal(facts.date, UNKNOWN_VALUE);
  assert.equal(facts.ownHero, UNKNOWN_VALUE);
  assert.equal(facts.duration, UNKNOWN_VALUE);
  assert.equal(facts.sizeBytes, 100);
  assert.equal(replayRowFacts(entry({ bundle_bytes: 0 })).sizeBytes, null);
});
