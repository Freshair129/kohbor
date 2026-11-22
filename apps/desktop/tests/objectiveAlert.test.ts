import assert from "node:assert/strict";
import { test } from "node:test";
import {
  createObjectiveAlertState,
  objectiveOverlayLayers,
  objectiveAlertText,
  objectiveAlertVisible,
  OBJECTIVE_ALERT_AUTO_HIDE_MS,
  parseObjectiveAlertBatch,
  receiveObjectiveAlert,
} from "../src/objectiveAlertModel.ts";
import { parseObjectivePrefs } from "../src/objectivePrefs.ts";
import { OBJECTIVE_COPY } from "../src/copy.ts";

const waterSpawn = {
  schema_version: 1,
  observed_clock_seconds: 120,
  alerts: [{ objective: "water_rune", phase: "spawn", occurrence_seconds: 120, due_seconds: 120 }],
};

test("objective alert parser accepts only closed host payloads", () => {
  assert.deepEqual(parseObjectiveAlertBatch(waterSpawn), waterSpawn);
  assert.equal(parseObjectiveAlertBatch({ ...waterSpawn, player_name: "must not be present" }), null);
  assert.equal(parseObjectiveAlertBatch({ ...waterSpawn, alerts: [{ ...waterSpawn.alerts[0], phase: "pre_warn_30" }] }), null);
});

test("objective alert text follows Thai and English app locales", () => {
  const state = receiveObjectiveAlert(createObjectiveAlertState(), waterSpawn, 100);
  assert.equal(objectiveAlertText(state, OBJECTIVE_COPY.th.alerts), "รูนน้ำเกิดแล้ว");
  assert.equal(objectiveAlertText(state, OBJECTIVE_COPY.en.alerts), "Water rune spawned");
});

test("objective alert auto-hides after three seconds", () => {
  const state = receiveObjectiveAlert(createObjectiveAlertState(), waterSpawn, 1_000);
  assert.equal(OBJECTIVE_ALERT_AUTO_HIDE_MS, 3_000);
  assert.equal(objectiveAlertVisible(state, 3_999), true);
  assert.equal(objectiveAlertVisible(state, 4_000), false);
});

test("objective text remains layered with a video banner and does not keep itself after expiry", () => {
  const state = receiveObjectiveAlert(createObjectiveAlertState(), waterSpawn, 1_000);
  assert.deepEqual(objectiveOverlayLayers(state, 1_500, true), {
    textAlert: true,
    videoBanner: true,
    visible: true,
  });
  assert.deepEqual(objectiveOverlayLayers(state, 4_000, true), {
    textAlert: false,
    videoBanner: true,
    visible: true,
  });
});

test("objective preferences require every known field and reject unrecognized keys", () => {
  const prefs = {
    schema_version: 1,
    water_rune: true,
    power_rune: true,
    bounty_rune: true,
    wisdom_rune: true,
    tormentor: true,
    diagnostic_map_keys: false,
  };
  assert.deepEqual(parseObjectivePrefs(prefs), prefs);
  assert.equal(parseObjectivePrefs({ ...prefs, extra: false }), null);
  assert.equal(parseObjectivePrefs({ ...prefs, tormentor: "yes" }), null);
});
