import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { test } from "node:test";
import {
  createObjectiveAlertState,
  objectiveAlertSentence,
  objectiveAlertVisible,
  objectiveCardContent,
  OBJECTIVE_ALERT_AUTO_HIDE_MS,
  parseObjectiveAlertBatch,
  receiveObjectiveAlert,
} from "../src/objectiveAlertModel.ts";
import { parseObjectivePrefs, validateLeadTimeInputs } from "../src/objectivePrefs.ts";
import { OBJECTIVE_COPY } from "../src/copy.ts";

const waterSpawn = {
  schema_version: 1,
  observed_clock_seconds: 120,
  alerts: [{ objective: "water_rune", phase: "spawn", lead_s: null, occurrence_seconds: 120, due_seconds: 120 }],
};

test("objective alert parser accepts only closed host payloads with phase lead metadata", () => {
  assert.deepEqual(parseObjectiveAlertBatch(waterSpawn), waterSpawn);
  assert.equal(parseObjectiveAlertBatch({ ...waterSpawn, player_name: "must not be present" }), null);
  assert.equal(parseObjectiveAlertBatch({ ...waterSpawn, alerts: [{ ...waterSpawn.alerts[0], lead_s: 30 }] }), null);
  assert.equal(parseObjectiveAlertBatch({ ...waterSpawn, alerts: [{ ...waterSpawn.alerts[0], phase: "pre_warn_30" }] }), null);
});

test("the host alert carries the app locale and the HUD card follows it (owner build b0b0745 showed English under th)", () => {
  const thai = parseObjectiveAlertBatch({ ...waterSpawn, locale: "th" })!;
  assert.equal(thai.locale, "th");
  assert.equal(parseObjectiveAlertBatch({ ...waterSpawn, locale: "en" })!.locale, "en");
  assert.equal(parseObjectiveAlertBatch(waterSpawn)!.locale, undefined);
  assert.equal(parseObjectiveAlertBatch({ ...waterSpawn, locale: "fr" }), null);
  assert.equal(parseObjectiveAlertBatch({ ...waterSpawn, locale: 1 }), null);
  assert.equal(objectiveCardContent(thai, thai.locale ?? "en").sentence, "Water rune เกิดแล้ว");
  const hudStage = readFileSync(new URL("../src/HudStage.tsx", import.meta.url), "utf8");
  assert.match(hudStage, /<ObjectiveAlert batch=\{objectiveAlert\.batch\} locale=\{objectiveAlert\.batch\?\.locale \?\? locale\}/);
});

test("objective card keeps English names with Thai sentences and fully English wording", () => {
  const spawned = parseObjectiveAlertBatch(waterSpawn)!.alerts[0]!;
  const prewarn = {
    objective: "water_rune" as const,
    phase: "pre_warn" as const,
    lead_s: 30,
    occurrence_seconds: 150,
    due_seconds: 120,
  };
  assert.equal(objectiveAlertSentence(spawned, "th"), "Water rune เกิดแล้ว");
  assert.equal(objectiveAlertSentence(prewarn, "th"), "Water rune จะเกิดใน 30 วินาที");
  assert.equal(objectiveAlertSentence(prewarn, "en"), "Water rune spawns in 30 s");
  assert.equal(objectiveAlertSentence({ ...spawned, objective: "tormentor" }, "en"), "Tormentor spawned");
  assert.deepEqual(objectiveCardContent({ ...waterSpawn, alerts: [spawned, { ...spawned, objective: "power_rune" }] }, "en"), {
    objective: "water_rune",
    sentence: "Water rune spawned",
    additionalCount: 1,
  });
});

test("objective alert auto-hides after three seconds", () => {
  const state = receiveObjectiveAlert(createObjectiveAlertState(), waterSpawn, 1_000);
  assert.equal(OBJECTIVE_ALERT_AUTO_HIDE_MS, 3_000);
  assert.equal(objectiveAlertVisible(state, 3_999), true);
  assert.equal(objectiveAlertVisible(state, 4_000), false);
});

test("each objective kind has its own inline original glyph", () => {
  const source = readFileSync(new URL("../src/ObjectiveAlert.tsx", import.meta.url), "utf8");
  for (const kind of ["water_rune", "power_rune", "bounty_rune", "wisdom_rune", "tormentor"]) {
    assert.match(source, new RegExp(`case "${kind}":[\\s\\S]*?<svg`), kind);
  }
  assert.doesNotMatch(source, /https?:\/\/|<img\b/u);
});

const prefsV2 = {
  schema_version: 2,
  objectives: {
    water_rune: { enabled: true, lead_s: [30, 15] },
    power_rune: { enabled: true, lead_s: [30, 15] },
    bounty_rune: { enabled: true, lead_s: [] },
    wisdom_rune: { enabled: true, lead_s: [] },
    tormentor: { enabled: true, lead_s: [] },
  },
  diagnostic_map_keys: false,
};

test("objective preferences require strict schema v2 nested preferences", () => {
  assert.deepEqual(parseObjectivePrefs(prefsV2), prefsV2);
  assert.equal(parseObjectivePrefs({ ...prefsV2, extra: false }), null);
  assert.equal(parseObjectivePrefs({ ...prefsV2, schema_version: 1 }), null);
  assert.equal(parseObjectivePrefs({ ...prefsV2, objectives: { ...prefsV2.objectives, power_rune: { enabled: true, lead_s: [30, 30] } } }), null);
  assert.equal(parseObjectivePrefs({ ...prefsV2, objectives: { ...prefsV2.objectives, tormentor: { enabled: true, lead_s: [4] } } }), null);
  assert.equal(parseObjectivePrefs({ ...prefsV2, objectives: { ...prefsV2.objectives, tormentor: { enabled: "yes", lead_s: [] } } }), null);
});

test("lead-time input validation reports whole, range and duplicate errors before saving", () => {
  assert.deepEqual(validateLeadTimeInputs(["15", "30"]), { values: [15, 30], error: null });
  assert.deepEqual(validateLeadTimeInputs(["", ""]), { values: [], error: null });
  assert.equal(validateLeadTimeInputs(["15.5", ""] ).error, "whole_seconds");
  assert.equal(validateLeadTimeInputs(["4", ""] ).error, "range");
  assert.equal(validateLeadTimeInputs(["30", "30"]).error, "duplicate");
  assert.match(OBJECTIVE_COPY.th.leadTimeWholeError, /จำนวนเต็ม/u);
  assert.match(OBJECTIVE_COPY.en.leadTimeRangeError, /5 to 60/u);
});
