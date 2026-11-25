import test from "node:test";
import assert from "node:assert/strict";
import {
  buildHeroBenchmarksViewModel,
  interpolatePercentile,
  observedHeroBenchmarkMetrics,
} from "../src/analytics/heroBenchmarksModel.mjs";

// Synthetic fixture shaped like the documented OpenDota benchmarks response.
const syntheticBenchmarks = {
  hero_id: 2,
  gold_per_min: [
    { percentile: 10, value: 300 },
    { percentile: 50, value: 450 },
    { percentile: 90, value: 600 },
  ],
};

test("hero benchmark interpolation is local, linear, and bounded by known points", () => {
  assert.equal(interpolatePercentile(375, syntheticBenchmarks.gold_per_min), 30);
  assert.equal(interpolatePercentile(250, syntheticBenchmarks.gold_per_min), 10);
  assert.equal(interpolatePercentile(700, syntheticBenchmarks.gold_per_min), 90);
  assert.equal(interpolatePercentile(null, syntheticBenchmarks.gold_per_min), null);
  assert.equal(interpolatePercentile(400, [{ percentile: 50, value: 10 }]), null);
});

test("malformed synthetic curves and unknown metrics stay unknown", () => {
  assert.equal(interpolatePercentile(5, [
    { percentile: 90, value: 2 },
    { percentile: 10, value: 8 },
  ]), null);
  const model = buildHeroBenchmarksViewModel({
    enabled: true,
    requestState: "ready",
    budgetUsed: 2,
    benchmarks: syntheticBenchmarks,
    observed: { gpm: 375, xpm: null },
    fetchedAt: 1_795_000_000_000,
  });
  assert.equal(model.state, "ready");
  assert.equal(model.metrics.find((metric) => metric.key === "gold_per_min").percentile, 30);
  assert.equal(model.metrics.find((metric) => metric.key === "xp_per_min").percentile, null);
  assert.equal(model.attribution, "ข้อมูลเทียบ: OpenDota");
  assert.equal(model.population, "ข้อมูลครอบคลุมทุกแรงก์ · คำนวณ percentile ในเครื่อง");
});

test("benchmark view model preserves disabled, loading, offline, budget, and unknown states", () => {
  const base = { budgetUsed: 1 };
  assert.equal(buildHeroBenchmarksViewModel({ ...base, enabled: false }).state, "disabled");
  assert.equal(buildHeroBenchmarksViewModel({ ...base, enabled: true, requestState: "loading" }).state, "loading");
  assert.equal(buildHeroBenchmarksViewModel({ ...base, enabled: true, requestState: "offline" }).state, "offline");
  assert.equal(buildHeroBenchmarksViewModel({ ...base, enabled: true, requestState: "budget_exhausted" }).state, "budget_exhausted");
  assert.equal(buildHeroBenchmarksViewModel({ ...base, enabled: true, requestState: "unknown" }).state, "unknown");
  assert.equal(buildHeroBenchmarksViewModel({ ...base, enabled: true, requestState: "ready", benchmarks: {} }).metrics.every((metric) => metric.percentile === null), true);
});

test("replay metrics use only available bundle values and keep missing duration unknown", () => {
  assert.deepEqual(observedHeroBenchmarkMetrics({
    rates: { gpm: 420, xpm: 510 },
    final: { lh: { value: 240 }, k: 12 },
    hero_damage: 24_000,
  }, 1_200), {
    gpm: 420,
    xpm: 510,
    lhpm: 12,
    hdpm: 1_200,
    kpm: 0.6,
  });
  assert.equal(observedHeroBenchmarkMetrics({ final: { lh: 240 } }, null).lhpm, null);
});
