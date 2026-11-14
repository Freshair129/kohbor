import assert from "node:assert/strict";
import { test } from "node:test";
import { deriveAlertStrip } from "../src/deck/alertModel.ts";
import { deriveOwnHeroViewModel } from "../src/deck/ownHeroModel.ts";
import type { RiskView } from "../src/live/types.ts";

const own = (percent: number | null) => ({ ...deriveOwnHeroViewModel(null), snapshotAvailability: "fresh" as const, hpPercent: percent });
const risk: RiskView = { state: null, lastVisionEvent: "enemy_missing" };

test("low HP uses inclusive catalog 20% and meter 30% boundaries only", () => {
  for (const [hp, want] of [[0, "เลือดต่ำ · อันตราย"], [20, "เลือดต่ำ · อันตราย"], [20.01, "เลือดต่ำ"], [30, "เลือดต่ำ"], [30.01, null], [null, null]] as const) {
    assert.equal(deriveAlertStrip(own(hp), "live", null, "th").health?.label ?? null, want);
  }
});

test("stale data suppresses current alerts; no vision keeps real fresh HP and explains missing capture", () => {
  const stale = deriveAlertStrip(own(10), "stale", risk, "th");
  assert.equal(stale.health, null); assert.equal(stale.stateText, "ข้อมูลเก่า"); assert.deepEqual(stale.vision, []);
  const noVision = deriveAlertStrip(own(25), "novision", risk, "th");
  assert.equal(noVision.health?.label, "เลือดต่ำ"); assert.equal(noVision.stateText, "ไม่มีภาพสด"); assert.deepEqual(noVision.vision, []);
  assert.equal(deriveAlertStrip({ ...own(10), snapshotAvailability: "unknown" }, "live", null, "th").health, null);
});

test("vision chips come only from the risk store and keep experimental words", () => {
  assert.deepEqual(deriveAlertStrip(own(100), "live", null, "th").vision, []);
  assert.match(deriveAlertStrip(own(100), "live", risk, "th").vision[0]!, /ศัตรูหาย.*ทดลอง/);
  assert.match(deriveAlertStrip(own(100), "live", { state: null, lastVisionEvent: "risk_alert" }, "th").vision[0]!, /ทดลอง/);
});
