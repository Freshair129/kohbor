import assert from "node:assert/strict";
import test from "node:test";
import {
  clampLayout,
  dragLayout,
  nudgeLayout,
  overlappingZones,
  parseLayoutView,
  parsePreview,
  rectOnCanvas,
  sameLayout,
  type Bounds,
  type HudZone,
} from "../src/lab/labLayout.ts";

const BOUNDS: Bounds = { x_percent: [0, 100], y_percent: [0, 100], scale: [0.2, 3] };
const BASE = { x_percent: 49.46, y_percent: 12.82, scale: 0.45, enabled: true };

test("clamp keeps every field inside the bounds and rounds to two decimals", () => {
  const clamped = clampLayout({ x_percent: 120, y_percent: -4, scale: 9, enabled: true }, BOUNDS);
  assert.deepEqual(clamped, { x_percent: 100, y_percent: 0, scale: 3, enabled: true });
  assert.equal(clampLayout({ ...BASE, scale: 0.0001 }, BOUNDS).scale, 0.2);
  assert.equal(clampLayout({ ...BASE, x_percent: 33.33333 }, BOUNDS).x_percent, 33.33);
  assert.equal(clampLayout({ ...BASE, x_percent: Number.NaN }, BOUNDS).x_percent, 50, "NaN falls back to centre");
});

test("drag converts pixel deltas on the canvas to percent of the screen and clamps", () => {
  const moved = dragLayout(BASE, 192, 54, 960, 540, BOUNDS);
  assert.equal(moved.x_percent, 70);
  assert.equal(moved.y_percent, 20.5);
  assert.equal(moved.scale, 0.45);
  const edge = dragLayout(BASE, 100000, -100000, 960, 540, BOUNDS);
  assert.equal(edge.x_percent, 100);
  assert.equal(edge.y_percent, 0);
  assert.deepEqual(dragLayout(BASE, 5, 5, 0, 0, BOUNDS), BASE, "a zero-size canvas changes nothing");
});

test("arrow keys nudge by 0.1 percent, shift by 1, other keys do nothing", () => {
  assert.equal(nudgeLayout(BASE, "ArrowRight", false, BOUNDS)?.x_percent, 50.1);
  assert.equal(nudgeLayout(BASE, "ArrowLeft", true, BOUNDS)?.x_percent, 49);
  assert.equal(nudgeLayout(BASE, "ArrowDown", false, BOUNDS)?.y_percent, 10.6);
  assert.equal(nudgeLayout(BASE, "ArrowUp", true, BOUNDS)?.y_percent, 9.5);
  assert.equal(nudgeLayout(BASE, "a", false, BOUNDS), null);
  assert.equal(nudgeLayout({ ...BASE, x_percent: 100 }, "ArrowRight", true, BOUNDS)?.x_percent, 100);
});

const ZONES: HudZone[] = [
  { name: "minimap", x0: 0, y0: 0.7, x1: 0.2, y1: 1 },
  { name: "top_bar", x0: 0, y0: 0, x1: 1, y1: 0.05 },
];

test("overlap is strict: touching edges do not count, real area does", () => {
  const monitor = { w: 1920, h: 1080 };
  // Stage entirely in the middle: no overlap.
  assert.deepEqual(overlappingZones({ x: 800, y: 300, w: 300, h: 90 }, monitor, ZONES), []);
  // Overlaps the top bar.
  assert.deepEqual(overlappingZones({ x: 800, y: 20, w: 300, h: 90 }, monitor, ZONES), ["top_bar"]);
  // Touches the top bar's lower edge (y = 54) exactly: not an overlap.
  assert.deepEqual(overlappingZones({ x: 800, y: 54, w: 300, h: 90 }, monitor, ZONES), []);
  // Both.
  assert.deepEqual(overlappingZones({ x: 100, y: 700, w: 400, h: 150 }, monitor, ZONES), ["minimap"]);
  assert.deepEqual(overlappingZones({ x: 0, y: 0, w: 1920, h: 1080 }, monitor, ZONES), ["minimap", "top_bar"]);
});

test("rectOnCanvas scales a monitor rectangle to canvas pixels", () => {
  assert.deepEqual(rectOnCanvas({ x: 960, y: 540, w: 480, h: 270 }, 1920, 960), {
    left: 480,
    top: 270,
    width: 240,
    height: 135,
  });
  assert.deepEqual(rectOnCanvas({ x: 0, y: 0, w: 10, h: 10 }, 0, 960), { left: 0, top: 0, width: 0, height: 0 });
});

test("sameLayout compares all four fields", () => {
  assert.equal(sameLayout(BASE, { ...BASE }), true);
  assert.equal(sameLayout(BASE, { ...BASE, enabled: false }), false);
  assert.equal(sameLayout(BASE, { ...BASE, scale: 0.46 }), false);
});

test("host views parse strictly", () => {
  const view = parseLayoutView({
    banner: BASE,
    default: BASE,
    bounds: { x_percent: [0, 100], y_percent: [0, 100], scale: [0.2, 3] },
    hud_zones: [{ name: "minimap", x0: 0, y0: 0.7, x1: 0.2, y1: 1 }],
    source: "embedded",
    override_path: null,
    applies: "on the next overlay show",
  });
  assert.ok(view);
  assert.equal(view.hudZones[0].name, "minimap");
  assert.equal(view.source, "embedded");
  assert.equal(parseLayoutView({ banner: { x_percent: "x" } }), null);
  assert.equal(parseLayoutView(null), null);

  const preview = parsePreview({
    disabled: false,
    window: { x: 10, y: 20, w: 300, h: 100 },
    stage: { x: 10, y: 20, w: 300, h: 92 },
  });
  assert.deepEqual(preview, { disabled: false, window: { x: 10, y: 20, w: 300, h: 100 }, stage: { x: 10, y: 20, w: 300, h: 92 } });
  assert.deepEqual(parsePreview({ disabled: true }), { disabled: true, window: null, stage: null });
  assert.equal(parsePreview({ disabled: false }), null);
});
