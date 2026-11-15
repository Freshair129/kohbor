import assert from "node:assert/strict";
import test from "node:test";
import {
  blockingGuards,
  formatValue,
  guardStatus,
  parseDescription,
  parseTrace,
  safetyReport,
} from "../src/lab/labModel.ts";

const DESCRIPTION = {
  profile_version: "1.0",
  taxonomy_version: "1.1",
  source: "embedded",
  embedded_default_sha256: "ab".repeat(32),
  overrides: ["health_danger.cooldown_ms"],
  override_path: null,
  rejected: null,
  events: {
    health_danger: {
      critical: true,
      params: {
        cooldown_ms: { value: 60000, default: 20000, min: 5000, max: 120000, unit: "ms", changed: true },
        window_ms: { value: 2000, default: 2000, min: 500, max: 5000, unit: "ms", changed: false },
      },
    },
    health_low: {
      critical: false,
      params: {
        threshold_percent: { value: 20, default: 20, min: 5, max: 50, unit: "percent", changed: false },
      },
    },
  },
};

test("unknown values are shown as a dash, never zero", () => {
  assert.equal(formatValue(null, "%"), "—");
  assert.equal(formatValue(0, "%"), "0 %");
  assert.equal(formatValue(20.625, "%"), "20.6 %");
  assert.equal(formatValue(1, "bool"), "yes");
  assert.equal(formatValue(0, "bool"), "no");
  assert.equal(formatValue(2000, "ms"), "2000 ms");
});

test("guard status distinguishes pass, fail and unknown", () => {
  assert.equal(guardStatus(true), "pass");
  assert.equal(guardStatus(false), "fail");
  assert.equal(guardStatus(null), "unknown");
});

test("trace parsing keeps nulls and rejects malformed rows", () => {
  const trace = parseTrace({
    at_ms: 10,
    status: "evaluated",
    events: [
      {
        id: "health_danger",
        fired: false,
        note: null,
        guards: [
          { name: "hp %", lhs: null, op: "<=", rhs: 25, unit: "%", pass: null },
          { name: "armed", lhs: 1, op: "==", rhs: 1, unit: "bool", pass: true },
        ],
      },
    ],
  });
  assert.ok(trace);
  assert.equal(trace.events[0].guards[0].lhs, null);
  assert.equal(trace.events[0].guards[0].pass, null);
  assert.equal(parseTrace({ at_ms: "x" }), null);
  assert.equal(parseTrace(null), null);
});

test("blocking guards are the failing and unknown ones, in order", () => {
  const trace = parseTrace({
    at_ms: 1,
    status: "evaluated",
    events: [
      {
        id: "health_danger",
        fired: false,
        note: null,
        guards: [
          { name: "hero alive", lhs: 1, op: "==", rhs: 1, unit: "bool", pass: true },
          { name: "hp %", lhs: 80, op: "<=", rhs: 25, unit: "%", pass: false },
          { name: "drop", lhs: null, op: ">=", rhs: 15, unit: "points", pass: null },
        ],
      },
    ],
  });
  assert.ok(trace);
  assert.deepEqual(
    blockingGuards(trace.events[0]).map((g) => g.name),
    ["hp %", "drop"],
  );
});

test("description parsing validates the shape", () => {
  const parsed = parseDescription(DESCRIPTION);
  assert.ok(parsed);
  assert.equal(parsed.events.health_danger.params.cooldown_ms.changed, true);
  assert.equal(parseDescription({}), null);
});

test("safety report lists critical events first and flags overrides on them", () => {
  const parsed = parseDescription(DESCRIPTION);
  assert.ok(parsed);
  const report = safetyReport(parsed);
  assert.equal(report[0].event, "health_danger");
  assert.deepEqual(report[0].changed, ["cooldown_ms"]);
  assert.equal(report[0].status, "overridden");
  assert.equal(report.length, 1, "non-critical events are not part of the safety strip");
});
