// Shared six-row hero-card contract for Replay, Analytics and Live.
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { test } from "node:test";
import { HERO_CARD_ROW_IDS, heroCardView } from "../src/analytics/heroCardView.mjs";

test("the shared hero-card view keeps six ordered rows and records unknown fields", () => {
  const view = heroCardView({ top: { name: "Ember", netWorth: null }, bars: { hp: 0, mana: "—" }, st: { str: 38, reg: Number.NaN } });
  assert.deepEqual(view.rows.map(row => row.id), ["top", "bars", "ab", "it", "st", "dv"]);
  assert.deepEqual(view.rows[0]?.unknownFields, ["netWorth"]);
  assert.deepEqual(view.rows[1]?.unknownFields, ["mana"]);
  assert.deepEqual(view.rows[4]?.unknownFields, ["reg"]);
  assert.equal(HERO_CARD_ROW_IDS.length, 6);
});

// Owner feedback 2026-11-25 on build 7d64957 ("ฮีโร่การ์ดของเพลเยอร์ก็เละ"): Live keeps its own approved card layout
// (own card in two stat rows, compact team cards); only Replay and Analytics share the six-row reference card.
test("Replay and Analytics render the canonical row IDs; Live keeps its own card layout", () => {
  const replay = readFileSync(new URL("../src/replay/matchViewer.mjs", import.meta.url), "utf8");
  const live = readFileSync(new URL("../src/live/HeroCard.tsx", import.meta.url), "utf8");
  const surface = readFileSync(new URL("../src/replay/MatchViewPanel.tsx", import.meta.url), "utf8");
  const analytics = readFileSync(new URL("../src/analytics/AnalyticsPage.tsx", import.meta.url), "utf8");
  assert.match(replay, /import \{ heroCardView \} from "\.\.\/analytics\/heroCardView\.mjs"/);
  for (const row of HERO_CARD_ROW_IDS) assert.ok(replay.includes(`rowAttr("${row}")`), `Replay ${row} row`);
  assert.doesNotMatch(live, /sharedHeroCardView\(view\)\.rows/, "Live does not use the six-row reference card");
  assert.match(analytics, /statLabelMode="text"/);
  assert.match(surface, /live\/heroCard\.css\?raw/);
});

test("the shared stylesheet locks the six-row reference geometry and compact scale", () => {
  const css = readFileSync(new URL("../src/live/heroCard.css", import.meta.url), "utf8");
  assert.match(css, /grid-template-areas:\s*"pt top" "pt bars" "ab ab" "it it" "st st" "dv dv"/);
  for (const size of ["46px", "26px", "31px", "23px", "20px", "15px", "24px", "13.5px", "10.5px", "10px", "9.5px", "9px"])
    assert.ok(css.includes(size), `reference size ${size}`);
  assert.match(css, /--hc-scale/);
  const analytics = readFileSync(new URL("../src/analytics/AnalyticsPage.tsx", import.meta.url), "utf8");
  assert.match(analytics, /"--hc-scale": layout\.heroCardScale/);
});

test("Replay always renders missing stats and derived cells as an em dash", () => {
  const replay = readFileSync(new URL("../src/replay/matchViewer.mjs", import.meta.url), "utf8");
  assert.match(replay, /tot === null \? "—"/);
  assert.match(replay, /<b>—<\/b>/);
  assert.doesNotMatch(replay, /function derivedHTML\(h, i, t, rowAttributes = ""\)\{\s*const d = derived\(h, t\); if \(!d\) return ""/);
});
