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

test("lane expectation readings preserve raw outcomes and label each side as trial", () => {
  const reading = exported("laneExpectationReading");
  const laneBadgeLabel = exported("laneBadgeLabel");
  const lane = {
    outcome: "dire_won",
    label: "ทดลอง · เกณฑ์ชั่วคราว",
    score: { value: 12.3 },
    expectation: { value: 5, source_label: "ตารางโค้ช · coach · owner · 7.42 · 2026-11-25" },
    verdict_by_side: {
      radiant: { label: "above", far: false, text_th: "รายละเอียดฝั่ง Radiant" },
      dire: { label: "below", far: true, text_th: "รายละเอียดฝั่ง Dire" },
    },
  };
  assert.equal(laneBadgeLabel(lane.outcome), "ชนะเลน · เกณฑ์ชั่วคราว");
  assert.deepEqual(reading(lane, "radiant"), {
    trialLabel: "ทดลอง · เกณฑ์ชั่วคราว", score: 12.3, expectation: 5,
    source: "ตารางโค้ช · coach · owner · 7.42 · 2026-11-25",
    verdict: "เกินคาด", explanation: "รายละเอียดฝั่ง Radiant", reason: null, personalAverage: null,
  });
  assert.equal(reading(lane, "dire").verdict, "ต่ำกว่าคาดมาก");
});

test("lanes without expectations fall back to the lane card and crowded strips omit the extra label", () => {
  const reading = exported("laneExpectationReading");
  const laneBadgeLabel = exported("laneBadgeLabel");
  const showLaneExpectationInStrip = exported("showLaneExpectationInStrip");
  const lane = { outcome: "even", score: { value: -7.3 }, expectation: null, expectation_reason: "no_reviewed_row" };
  assert.equal(reading(lane, "radiant").expectation, null);
  assert.equal(reading(lane, "radiant").verdict, null);
  assert.equal(reading(lane, "radiant").reason, "no_reviewed_row");
  assert.equal(laneBadgeLabel(lane.outcome), "สูสี · เกณฑ์ชั่วคราว");
  assert.equal(showLaneExpectationInStrip([{}, {}, {}, {}], 4), true);
  assert.equal(showLaneExpectationInStrip([{}, {}, {}, {}, {}, {}], 4), false);
  assert.equal(showLaneExpectationInStrip([{}], null), false);
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

test("lane card and matchup strip show trial readings without unsafe HTML sinks", async () => {
  const source = await readFile(new URL("../src/analytics/knowledge/index.tsx", import.meta.url), "utf8");
  assert.match(source, /คะแนนเลน \(เกณฑ์ของเกาะเบาะ\)/);
  assert.match(source, /ยังไม่มีค่าคาดหมาย/);
  assert.match(source, /ค่าเฉลี่ยของคุณ/);
  assert.match(source, /lane_strip_max_cells/);
  assert.match(source, /radiantReading\.verdict[^\n]*radiantReading\.trialLabel/);
  assert.match(source, /reading\.verdict\} · \{reading\.trialLabel\}/);
  assert.doesNotMatch(source, /dangerouslySetInnerHTML/);
});

test("coach review page enforces each server-provided blind stage", async () => {
  const source = await readFile(new URL("../src/analytics/LaneReviewPage.tsx", import.meta.url), "utf8");
  assert.match(source, /review\.stage === "blind"/);
  assert.match(source, /review\.stage === "half_blind"/);
  assert.match(source, /review\.stage === "revealed"/);
  assert.match(source, /analytics_lane_review_submit_blind/);
  assert.match(source, /analytics_lane_review_submit_half_blind/);
  assert.match(source, /analytics_lane_review_next/);
  assert.match(source, /trial_label/);
});
