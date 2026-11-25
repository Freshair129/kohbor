import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { test } from "node:test";
import * as analytics from "../src/analytics/analyticsModel.ts";
import { createAnalyticsChartResolver } from "../src/analytics/analyticsChart.ts";

test("Analytics selects docked, drawer and enlarge modes from measured page sizes", () => {
  const expected = [
    [1366, "drawer", 289],
    [1440, "drawer", 289],
    [1600, "drawer", 423],
    [1920, "docked", 423],
    [2560, "docked", 423],
  ] as const;
  for (const [width, mode, chatWidth] of expected) {
    const layout = analytics.analyticsLayout(width, 900);
    assert.equal(layout.mode, mode, `${width}px page width`);
    assert.equal(layout.right, chatWidth, `${width}px chat width`);
    assert.equal(layout.overlap, false, `${width}px must not squeeze the viewer`);
  }
  assert.equal(analytics.analyticsLayout(1200, 900).mode, "enlarge-window");
  assert.equal(analytics.analyticsLayout(1920, 700).mode, "enlarge-window");
});

test("Analytics exposes exactly the four approved short tabs", () => {
  assert.deepEqual(analytics.ANALYTICS_TABS.map(([, label]) => label), ["สรุป", "เทียบ", "AI", "โน้ต"]);
});

test("old provider consent opens once and current consent does not re-prompt", () => {
  const oldConsent = { text_version: 2 };
  assert.deepEqual(analytics.analyticsConsentRePrompt(oldConsent, 3), {
    showBanner: true, openDialog: true, noticeKey: "2:3",
  });
  assert.deepEqual(analytics.analyticsConsentRePrompt(oldConsent, 3, true), {
    showBanner: false, openDialog: false, noticeKey: "2:3",
  });
  assert.equal(analytics.analyticsConsentRePrompt({ text_version: 3 }, 3).showBanner, false);
});

test("selecting a library entry never requests parsing and leaves parsing to an explicit action", () => {
  for (const entry of [
    { match_id: "available-1", status: { state: "available" } },
    { match_id: "failed-1", status: { state: "failed", reason: "ENGINE_EXIT" } },
  ]) {
    assert.deepEqual(analytics.analyticsReplaySelection(entry), {
      selectedId: entry.match_id, parseOnSelect: false,
    });
  }
  assert.equal(analytics.analyticsReplayAction({ state: "available" }), "parse");
  assert.equal(analytics.analyticsReplayAction({ state: "failed", reason: "ENGINE_EXIT" }), "retry");
  assert.equal(analytics.analyticsReplayAction({ state: "needs_pack" }), "retry");
  assert.equal(analytics.analyticsReplayAction({ state: "queued" }), null);
  assert.equal(analytics.analyticsReplayAction({ state: "parsing" }), null);
});

test("bundle reload identity changes only with selected replay, bundle hashes or state", () => {
  // List replies carry bundle_hash, not source or engine hashes (R-22).
  const entry = { match_id: "m1", status: { state: "ready" }, bundle_hash: "bundle-a", bundle_schema: "3.1" } as const;
  const original = analytics.analyticsBundleSelectionKey(entry);
  assert.equal(analytics.analyticsBundleSelectionKey({ ...entry }), original);
  assert.notEqual(analytics.analyticsBundleSelectionKey({ ...entry, bundle_schema: "3.0" }), original);
  assert.notEqual(analytics.analyticsBundleSelectionKey({ ...entry, bundle_hash: "bundle-b" }), original);
  assert.notEqual(analytics.analyticsBundleSelectionKey({ ...entry, status: { state: "parsing" } }), original);
  assert.notEqual(analytics.analyticsBundleSelectionKey({ ...entry, match_id: "m2" }), original);
});

test("untested provider controls and chat visibility follow Developer mode", () => {
  assert.deepEqual(analytics.analyticsAiVisibility(true, true), {
    showUntestedControl: true, showUntested: true, chatCollapsed: false, showUnavailableNotice: false,
  });
  assert.deepEqual(analytics.analyticsAiVisibility(false, true), {
    showUntestedControl: false, showUntested: false, chatCollapsed: true, showUnavailableNotice: true,
  });
});

test("match data deletion needs an explicit confirm action", () => {
  const opened = analytics.analyticsDeleteTransition("closed", "request", "match-1");
  assert.deepEqual(opened, { state: "confirming", command: null });
  assert.deepEqual(analytics.analyticsDeleteTransition(opened.state, "cancel", "match-1"), {
    state: "closed", command: null,
  });
  assert.deepEqual(analytics.analyticsDeleteTransition(opened.state, "confirm", "match-1"), {
    state: "deleting", command: { name: "analytics_match_data_delete", args: { matchId: "match-1" } },
  });
  assert.equal(analytics.ANALYTICS_ORPHANS_COMMAND, "analytics_orphans_list");
  const page = readFileSync(new URL("../src/analytics/AnalyticsPage.tsx", import.meta.url), "utf8");
  const deleteFlow = page.match(/async function deleteSelectedAnalysis\(\) \{([\s\S]*?)\n  \}/)?.[1] ?? "";
  const model = readFileSync(new URL("../src/analytics/analyticsModel.ts", import.meta.url), "utf8");
  assert.match(model, /name: "analytics_match_data_delete"/);
  assert.match(deleteFlow, /invoke\(transition\.command\.name, transition\.command\.args\)/);
  assert.match(page, /className="analytics-delete-confirm"/);
  assert.doesNotMatch(deleteFlow, /window\.confirm/);
});

test("reference charts resolve only points present in the selected match and digest", () => {
  const resolve = createAnalyticsChartResolver({ match: {
    t0: 0, t1: 2,
    heroes: [{ id: "npc_dota_hero_axe", name: "Axe", eco: [[20, 100, 1, 0], [25, 120, 2, 0], [30, 140, 3, 0]] }],
  } }, { advantage_per_min: [{ minute: 0, radiant_nw_diff: 50, radiant_xp_diff: null }] });
  assert.deepEqual(resolve({ ref: "hero_series", hero: "axe", metric: "nw" }), {
    name: "Axe · nw", points: [{ x: 0, y: 100 }, { x: 1, y: 120 }, { x: 2, y: 140 }],
  });
  assert.deepEqual(resolve({ ref: "advantage_per_min", metric: "nw" }), {
    name: "ส่วนต่างมูลค่าสุทธิ · Radiant", points: [{ x: 0, y: 50 }],
  });
  assert.equal(resolve({ ref: "hero_series", hero: "missing", metric: "nw" }), null);
});

test("selection toolbar listens on the document, not only inside the quotable pane (88f4276 c9)", async () => {
  const { readFileSync } = await import("node:fs");
  const page = readFileSync(new URL("../src/analytics/AnalyticsPage.tsx", import.meta.url), "utf8");
  assert.match(page, /document\.addEventListener\("mouseup", onRelease\)/);
  assert.match(page, /closest<HTMLElement>\("\[data-analytics-quotable\]"\)/);
  assert.doesNotMatch(page, /data-analytics-quotable onMouseUp=/);
});

test("overview best-hero panels sit two per row with today on its own row", async () => {
  const { readFileSync } = await import("node:fs");
  const css = readFileSync(new URL("../src/overview/overview.css", import.meta.url), "utf8");
  assert.match(css, /\.ov-best-row \{ display: grid; grid-template-columns: repeat\(2, minmax\(0, 1fr\)\)/);
  assert.match(css, /\.ov-best-row > \.ov-best-today \{ grid-column: 1 \/ -1; \}/);
});
