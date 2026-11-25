import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { test } from "node:test";
import * as analyticsModel from "../src/analytics/analyticsModel.ts";

function closeTo(actual: number, expected: number, tolerance = 2) {
  assert.ok(Math.abs(actual - expected) <= tolerance, `${actual} is not within ${tolerance} of ${expected}`);
}

test("Analytics keeps the approved chat width and scales only the hero cards at both reference viewports", () => {
  for (const [width, height, before, after] of [
    [1276, 900, { docs: 28, centre: 939, chat: 289, scale: 0.88 }, { docs: 28, centre: 939, chat: 289, scale: 0.88 }],
    [1756, 1080, { docs: 273, centre: 1040, chat: 423, scale: 1 }, { docs: 273, centre: 1040, chat: 423, scale: 1 }],
  ] as const) {
    const layout = analyticsModel.analyticsLayout(width, height);
    closeTo(layout.docs, before.docs);
    closeTo(layout.center, before.centre);
    closeTo(layout.right, before.chat);
    closeTo(layout.heroCardScale, before.scale, 0.001);
    assert.equal(layout.rightRail, 44);
    assert.equal(layout.overlap, false, `${width}x${height}`);
    assert.equal(layout.pageScroll, false);
    assert.deepEqual(after, before);
  }
});

test("The 12 px floor remains outside the hero-card exemption and shared card scale stays uniform", () => {
  for (const path of ["../src/analytics/analytics.css", "../src/replay/matchViewer.css"]) {
    const css = readFileSync(new URL(path, import.meta.url), "utf8");
    const sizes = [...css.matchAll(/font(?:-size)?\s*:\s*([^;{}]+)/g)]
      .map(match => Number(match[1]?.match(/(\d+(?:\.\d+)?)px/)?.[1]))
      .filter(Number.isFinite);
    assert.ok(sizes.length > 0, path);
    assert.ok(sizes.every(size => size >= 12), `${path} has fonts below 12 px: ${sizes.filter(size => size < 12).join(", ")}`);
  }
  const shared = readFileSync(new URL("../src/live/heroCard.css", import.meta.url), "utf8");
  assert.match(shared, /--hc-scale/);
  assert.match(shared, /\.rc \.bar \.num\s*\{[^}]*font:\s*600 9\.5px/s);
  assert.match(shared, /\.rc \.row\.dvs\s*\{[^}]*grid-template-columns:\s*repeat\(4,\s*minmax\(0,\s*1fr\)\)/s);
  assert.match(shared, /\.rc\s*\{[^}]*transform-origin:\s*top left/s);
  const viewer = readFileSync(new URL("../src/replay/matchViewer.mjs", import.meta.url), "utf8");
  assert.match(viewer, /data-stat-label="text"/);
});

test("Analytics playback model formats time, exposes requested rates, and seeks five seconds by key", () => {
  assert.deepEqual(analyticsModel.ANALYTICS_PLAYBACK_SPEEDS, [1, 5, 10, 30]);
  assert.equal(analyticsModel.formatAnalyticsTime(0), "00:00");
  assert.equal(analyticsModel.formatAnalyticsTime(34 * 60 + 35), "34:35");
  assert.equal(analyticsModel.formatAnalyticsTime(null), "— ไม่ทราบ");
  assert.deepEqual(analyticsModel.analyticsPlaybackKeyAction(" "), { type: "toggle" });
  assert.deepEqual(analyticsModel.analyticsPlaybackKeyAction("ArrowLeft"), { type: "seek", deltaSeconds: -5 });
  assert.deepEqual(analyticsModel.analyticsPlaybackKeyAction("ArrowRight"), { type: "seek", deltaSeconds: 5 });
  assert.equal(analyticsModel.analyticsPlaybackKeyAction(" ", "select"), null);
  assert.equal(analyticsModel.analyticsPlaybackKeyAction(" ", "button"), null);
});

test("Analytics death ticks use the recorded victim team and keep unknown teams uncoloured", () => {
  const heroes = [{ id: "npc_dota_hero_axe", team: 2 }, { id: "npc_dota_hero_lina", team: 3 }];
  assert.equal(analyticsModel.analyticsDeathTeam({ hero: "npc_dota_hero_axe" }, heroes), 2);
  assert.equal(analyticsModel.analyticsDeathTeam({ hero: "npc_dota_hero_lina" }, heroes), 3);
  assert.equal(analyticsModel.analyticsDeathTeam({ hero: "npc_dota_hero_unknown" }, heroes), null);
});

test("Analytics detail route carries the loaded bundle and restores match, time, and open document", () => {
  const bundle = { match: { id: "bundle-1" } };
  const context = { matchId: "match-1", timeS: 975, documentId: "fight-notes.md" };
  const route = analyticsModel.enterAnalyticsReplayDetails(bundle, context);
  assert.equal(route.bundle, bundle);
  assert.equal(route.matchId, context.matchId);
  assert.equal(route.initialTime, context.timeS);
  assert.deepEqual(analyticsModel.returnToAnalytics(route), context);
});

test("Analytics detail route restores the AI report in the left panel", () => {
  const page = readFileSync(new URL("../src/analytics/AnalyticsPage.tsx", import.meta.url), "utf8");
  assert.match(page, /documentId:\s*knowledgeTab === "ai" \? "ai-report" : openNote/);
  assert.match(page, /restoreContext\.documentId === "ai-report"[\s\S]*?setKnowledgeTab\("ai"\)/);
});

test("Analytics viewer leaves item, ability, and buff rows visible and hosts its player under the map", () => {
  const panel = readFileSync(new URL("../src/replay/MatchViewPanel.tsx", import.meta.url), "utf8");
  const viewer = readFileSync(new URL("../src/replay/matchViewer.mjs", import.meta.url), "utf8");
  const markup = readFileSync(new URL("../src/replay/matchViewer.html", import.meta.url), "utf8");
  const analytics = readFileSync(new URL("../src/analytics/AnalyticsPage.tsx", import.meta.url), "utf8");
  const overrides = panel.match(/const analyticsSurfaceOverrides = `([\s\S]*?)`;/)?.[1] ?? "";
  assert.match(overrides, /minmax\(calc\(344px \* var\(--hc-scale, 1\)\)/);
  assert.doesNotMatch(overrides, /\.rc(?:\s|\{|\.)/);
  assert.match(panel, /live\/heroCard\.css\?raw/);
  assert.match(panel, /type HeroCardLabelMode = "text" \| "icon"/);
  assert.match(analytics, /statLabelMode="text"/);
  assert.match(viewer, /statLabelMode/);
  assert.match(analytics, /getElementById\("analytics-player-slot"\)/);
  assert.match(markup, /id="analytics-player-slot"/);
  assert.match(viewer, /class="row ab" \$\{rowAttr\("ab"\)\}>\$\{abil\}<span class="mods">/);
  assert.match(viewer, /class="row it" \$\{rowAttr\("it"\)\}>\$\{\[0, 1, 2, 3, 4, 5\]/);
});

test("Analytics detail uses the selected bundle and time, then exposes its restore control without the report picker", () => {
  const app = readFileSync(new URL("../src/App.tsx", import.meta.url), "utf8");
  const replayPanel = readFileSync(new URL("../src/replay/ReplayPanel.tsx", import.meta.url), "utf8");
  const matchPanel = readFileSync(new URL("../src/replay/MatchViewPanel.tsx", import.meta.url), "utf8");
  assert.match(app, /onOpenReplay=\{openAnalyticsDetails\}/);
  assert.match(app, /analyticsReturnContext\(analyticsDetail\)/);
  assert.match(replayPanel, /detail\.bundle/);
  assert.match(replayPanel, /detail\.initialTime/);
  assert.match(replayPanel, /analyticsPlayer\.back/);
  assert.match(replayPanel, /initialBundle=\{detail\.bundle\}/);
  assert.match(matchPanel, /detailOnly/);
  assert.match(replayPanel, /if \(detail\) return/);
});

test("right rail is 44 px, retains the avatar and wraps its 12 px friend label", () => {
  const css = readFileSync(new URL("../src/styles.css", import.meta.url), "utf8");
  const rail = readFileSync(new URL("../src/shell/RailRight.tsx", import.meta.url), "utf8");
  assert.match(css, /\.app\s*\{\s*--rail-width:\s*44px;\s*\}/);
  assert.match(css, /\.shell-rail-right \.shell-account-trigger\s*\{[^}]*min-width:\s*0[^}]*padding:\s*0/s);
  assert.match(css, /\.shell-rail-right \.shell-rail-friends-note\s*\{[^}]*font-size:\s*12px/s);
  assert.match(css, /\.shell-rail-right \.shell-rail-friends-note\s*\{[^}]*overflow-wrap:\s*anywhere/s);
  assert.match(css, /\.shell-account-avatar\s*\{[^}]*width:\s*38px/s);
  assert.match(rail, /title=\{labels\.account\}/);
  assert.match(rail, /title=\{labels\.friendsNotYet\}/);
});
