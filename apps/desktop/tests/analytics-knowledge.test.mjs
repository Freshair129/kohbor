import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";
import * as model from "../src/analytics/knowledge/presentationModel.mjs";

function exported(name) {
  assert.equal(typeof model[name], "function", `${name} should be exported`);
  return model[name];
}

test("line chart geometry preserves unknown values as gaps", () => {
  const buildLineSegments = exported("buildLineSegments");
  const segments = buildLineSegments([2, null, -2], 100, 60);
  assert.deepEqual(segments, [
    [{ x: 0, y: 0 }],
    [{ x: 100, y: 60 }],
  ]);
});

test("matchup rows keep unknown lanes in a neutral group", () => {
  const matchupRows = exported("matchupRows");
  const rows = matchupRows([
    { id: "radiant-mid", side: "radiant", lane: "mid", slot: 2 },
    { id: "dire-mid", side: "dire", lane: "mid", slot: 1 },
    { id: "dire-unknown", side: "dire", lane: null, slot: 4 },
  ]);
  assert.deepEqual(rows.map((row) => row.lane), ["top", "mid", "bot", "unknown"]);
  assert.deepEqual(rows[1].heroes.map((hero) => hero.id), ["radiant-mid", "dire-mid"]);
  assert.equal(rows[3].heroes[0].id, "dire-unknown");
});

test("provisional lane outcomes are labeled as provisional", () => {
  const laneBadgeLabel = exported("laneBadgeLabel");
  assert.equal(laneBadgeLabel("radiant_won"), "ชนะเลน · เกณฑ์ชั่วคราว");
  assert.equal(laneBadgeLabel(null), "— ไม่ทราบ");
});

test("report staleness returns every current mismatch reason", () => {
  const reportStaleReasons = exported("reportStaleReasons");
  assert.deepEqual(
    reportStaleReasons(
      { bundle_hash: "old", knowledge_version: 1, prompt_version: 2, own_hero_id: "axe" },
      { bundle_hash: "new", knowledge_version: 2, prompt_version: 3, own_hero_id: "lina" },
    ),
    ["bundle_changed", "knowledge_changed", "prompt_changed", "own_hero_changed"],
  );
});

test("AI report generation is wired only to explicit generate and regenerate clicks", async () => {
  const source = await readFile(new URL("../src/analytics/knowledge/index.tsx", import.meta.url), "utf8");
  assert.doesNotMatch(source, /\buse(?:Layout)?Effect\b/);
  assert.match(source, /onClick=\{report \? onRegenerateReport \?\? onGenerateReport : onGenerateReport\}/);
  assert.match(source, /report \? "สร้างใหม่" : "สร้างบทวิเคราะห์"/);
});
