import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { test } from "node:test";
import { analyticsLayout, analyticsViewState, UNKNOWN_VALUE, replayRowFacts } from "../src/analytics/analyticsModel.ts";
import type { ReplayLibraryEntry } from "../src/replay/replayLibraryClient.ts";

const entry = (partial: Partial<ReplayLibraryEntry> = {}): ReplayLibraryEntry => ({
  match_id: "1", match_key: null, source: "dota_replays_folder",
  engine_version: null, parse_time_unix_ms: null,
  status: { state: "ready" }, bundle_bytes: 100, storage_bytes: 150,
  created_at_unix_ms: 1, match_date_utc: null, own_hero: null, duration_s: null, patch: null,
  bundle_hash: null,
  ...partial,
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

test("Analytics fits the measured page sizes for both reference viewports without a page scroll", () => {
  for (const [width, height, expectedDocs, expectedCenter, expectedChat] of [[1276, 900, 28, 939, 289], [1756, 1080, 273, 1040, 423]] as const) {
    const layout = analyticsLayout(width, height);
    assert.equal(layout.overlap, false, `${width}x${height}`);
    assert.equal(layout.docs, expectedDocs);
    assert.equal(layout.center, expectedCenter);
    assert.equal(layout.right, expectedChat);
    assert.equal(layout.pageScroll, false);
  }
  const collapsed = analyticsLayout(1276, 900, true, true);
  assert.ok(collapsed.center > analyticsLayout(1276, 900).center);
  assert.equal(collapsed.mapMin, 300);
  assert.equal(analyticsLayout(1276, 900).mapMin, 300);
});

test("Analytics CSS uses measured layout tracks and a drawer for the compact document panel", () => {
  const css = readFileSync(new URL("../src/analytics/analytics.css", import.meta.url), "utf8");
  assert.match(css, /grid-template-columns:\s*var\(--analytics-docs-track,273px\)\s+minmax\(var\(--analytics-center-min,938px\),1fr\)\s+var\(--analytics-chat-track,423px\)/);
  assert.match(css, /\.analytics-page\[data-layout="drawer"\] \.analytics-docs\s*\{[^}]*position:\s*absolute/s);
  assert.match(css, /--hc-scale/);
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

test("AI submit sends the current composer prompt, selected hero, scrubber time and selected match", () => {
  const page = readFileSync(new URL("../src/analytics/AnalyticsPage.tsx", import.meta.url), "utf8");
  const request = page.match(/invoke<string>\("analytics_chat_submit",\s*\{([^}]*)\}/)?.[1] ?? "";
  assert.match(request, /matchId:\s*selectedId/);
  assert.match(page, /selectionPrompt\(question, quoteChips, locale\)/);
  assert.match(request, /question:\s*promptQuestion/);
  assert.match(request, /selectedHero:\s*hero/);
  assert.match(request, /scrubberTimeS:\s*currentTime/);
  assert.doesNotMatch(request, /include_|openDocument|documentText|contextChips/i);
});

test("documents route to the AI report or a saved note without opening external content", async () => {
  const model = await import("../src/analytics/analyticsModel.ts");
  assert.equal(typeof model.analyticsDocumentDestination, "function");
  if (typeof model.analyticsDocumentDestination === "function") {
    assert.equal(model.analyticsDocumentDestination("ai-report", ["note-1"]), "report");
    assert.equal(model.analyticsDocumentDestination("note-1", ["note-1"]), "note");
    assert.equal(model.analyticsDocumentDestination("../outside", ["note-1"]), null);
    assert.equal(model.analyticsDocumentDestination("unknown", ["note-1"]), null);
  }
});

test("history recap is a bounded local summary of visible chat turns", async () => {
  const model = await import("../src/analytics/analyticsModel.ts");
  assert.equal(typeof model.buildChatRecap, "function");
  if (typeof model.buildChatRecap === "function") {
    const recap = model.buildChatRecap([
      { role: "user", content: "Why was the mid lane lost?" },
      { role: "assistant", content: "At 10:00, net worth was behind. This came from the local digest." },
    ], 800);
    assert.match(recap, /created locally without an AI request/i);
    assert.match(recap, /Why was the mid lane lost\?/);
    assert.match(recap, /net worth was behind/);
    assert.ok(recap.length <= 800);
  }
});

test("Analytics mounts knowledge/report commands and transcript restart behind explicit actions", () => {
  const page = readFileSync(new URL("../src/analytics/AnalyticsPage.tsx", import.meta.url), "utf8");
  for (const component of ["KnowledgeTabs", "MatchSummaryTab", "HeroComparisonTab", "AiReportTab"]) {
    assert.ok(page.includes(`<${component}`), `${component} should be mounted`);
  }
  assert.doesNotMatch(page, /<MatchupStrip/, "D2 removes the lane strip from the replay stage");
  assert.match(page, /<MatchViewerSurface/, "the square map viewer remains the centre stage");
  for (const command of ["analytics_knowledge_get", "analytics_saved_analysis_read", "analytics_report_generate", "analytics_note_save_text", "analytics_chat_start_from_summary"]) {
    assert.match(page, new RegExp(command), `${command} should be connected`);
  }
  assert.match(page, /chat_history_over_budget/);
  assert.match(page, /action\.id === "ai-report"/);
  assert.match(page, /async function generateReport/);
});

test("chat tool activity uses the host event and renders one friendly line per started call", () => {
  const page = readFileSync(new URL("../src/analytics/AnalyticsPage.tsx", import.meta.url), "utf8");
  assert.match(page, /listen<ToolCallPayload>\("analytics:\/\/tool-activity"/);
  assert.match(page, /event\.payload\.status !== "started"/);
  assert.match(page, /toolCallLine\(event\.payload\.tool/);
});

test("orphan strings from the host become safe generic rows without displaying opaque keys", () => {
  const page = readFileSync(new URL("../src/analytics/AnalyticsPage.tsx", import.meta.url), "utf8");
  assert.match(page, /typeof item === "string"/);
  assert.match(page, /Orphaned analysis data/);
});

test("host registers same-match transcript and recap-gated history reset commands", () => {
  const commands = readFileSync(new URL("../src-tauri/src/commands.rs", import.meta.url), "utf8");
  const lib = readFileSync(new URL("../src-tauri/src/lib.rs", import.meta.url), "utf8");
  assert.ok(commands.includes("fn analytics_chat_transcript"));
  assert.ok(commands.includes("chat_transcript(&match_id)"));
  assert.ok(commands.includes("fn analytics_chat_start_from_summary"));
  assert.ok(commands.includes("start_new_chat_from_summary"));
  assert.equal((lib.match(/commands::analytics_chat_transcript/g) ?? []).length, 2);
  assert.equal((lib.match(/commands::analytics_chat_start_from_summary/g) ?? []).length, 2);
});

test("missing replay facts remain unknown and never become zero", () => {
  const facts = replayRowFacts(entry());
  assert.equal(facts.date, UNKNOWN_VALUE);
  assert.equal(facts.ownHero, UNKNOWN_VALUE);
  assert.equal(facts.duration, UNKNOWN_VALUE);
  assert.equal(facts.sizeBytes, 100);
  assert.equal(replayRowFacts(entry({ bundle_bytes: 0 })).sizeBytes, null);
});
