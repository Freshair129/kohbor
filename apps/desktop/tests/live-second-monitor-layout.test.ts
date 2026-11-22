import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { test } from "node:test";
import {
  MAP_MIN,
  ROW_CARD_MIN,
  TILE_CARD_MIN,
  placedRegions,
  secondMonitorLayout,
  type PlacedLayout,
  type Rect,
  // @ts-ignore Node's built-in TypeScript loader requires the explicit source extension.
} from "../src/live/secondMonitorLayout.ts";

// Live box = window minus the 88 px nav rail and the 28 px footer (friends rail folded during a match).
const box = (w: number, h: number) => ({ width: w - 88, height: h - 28 });
const WINDOWS: [string, number, number][] = [
  ["1920x1080", 1920, 1080], ["960x1080 half", 960, 1080], ["1080x1920 portrait", 1080, 1920], ["1280x720", 1280, 720],
  ["2560x1440", 2560, 1440], ["1366x768", 1366, 768], ["1100x700", 1100, 700], ["1080x960", 1080, 960],
  ["1600x900", 1600, 900], ["800x700", 800, 700], ["688x680 minimum", 688, 680],
];

function placed(side: "left" | "right", w: number, h: number): PlacedLayout {
  const layout = secondMonitorLayout({ side, ...box(w, h) });
  assert.ok(layout.mode === "wide" || layout.mode === "stacked", `${w}x${h}: expected a placed layout, got ${layout.mode}`);
  return layout as PlacedLayout;
}
const inside = (r: Rect, w: number, h: number) => r.x >= 0 && r.y >= 0 && r.x + r.w <= w && r.y + r.h <= h && r.w > 0 && r.h > 0;
const overlap = (a: Rect, b: Rect) => Math.min(a.x + a.w, b.x + b.w) - Math.max(a.x, b.x) > 0 && Math.min(a.y + a.h, b.y + b.h) - Math.max(a.y, b.y) > 0;

test("every supported window: all regions inside the box, none overlapping, our five cards complete at or above the floor", () => {
  for (const [name, w, h] of WINDOWS) {
    for (const side of ["left", "right"] as const) {
      const layout = placed(side, w, h);
      const { width, height } = box(w, h);
      const regions = placedRegions(layout);
      for (const [region, rect] of regions) assert.ok(inside(rect, width, height), `${name} ${side}: ${region} outside the box ${JSON.stringify(rect)}`);
      for (let i = 0; i < regions.length; i++) for (let j = i + 1; j < regions.length; j++) {
        assert.ok(!overlap(regions[i][1], regions[j][1]), `${name}: ${regions[i][0]} overlaps ${regions[j][0]}`);
      }
      const floor = layout.cardKind === "row" ? ROW_CARD_MIN : TILE_CARD_MIN;
      assert.ok(layout.card.w >= floor.w && layout.card.h >= floor.h, `${name}: card ${JSON.stringify(layout.card)} below the floor`);
      assert.ok(layout.map.w >= MAP_MIN && layout.map.w === layout.map.h, `${name}: map ${layout.map.w}`);
      // Five cards and the heading fit inside the team region (row: stacked vertically; tile: one row).
      const need = layout.cardKind === "row" ? 22 + 6 + 5 * layout.card.h + 4 * 6 : 22 + 6 + layout.card.h;
      assert.ok(need <= layout.ours.h, `${name}: our cards need ${need} > ${layout.ours.h}`);
      if (layout.cardKind === "row") assert.ok(layout.card.w <= layout.ours.w, `${name}: card wider than its column`);
      else assert.ok(5 * layout.card.w + 4 * 6 <= layout.ours.w, `${name}: five tiles do not fit the row`);
      assert.equal(layout.map.x, 0, "the map touches the inner edge");
      assert.equal(layout.band.y, layout.map.y + layout.map.h, "the quick strip sits directly under the map");
      assert.equal(layout.band.w, layout.map.w);
    }
  }
});

test("the four reviewed windows choose the proposed modes", () => {
  const full = placed("right", 1920, 1080);
  assert.deepEqual([full.mode, full.plan, full.enemy !== null, full.slot !== null], ["wide", "both+slot", true, true]);
  const half = placed("right", 960, 1080);
  assert.deepEqual([half.mode, half.enemy !== null, half.slot !== null], ["stacked", true, true]);
  assert.ok(half.enemy!.y < half.map.y && half.ours.y > half.band.y, "half: enemy row above the map, our row below");
  const portrait = placed("left", 1080, 1920);
  assert.deepEqual([portrait.mode, portrait.mapEdge, portrait.slot !== null], ["stacked", "right", true]);
  assert.ok(portrait.slot!.x > portrait.map.x + portrait.map.w, "the free slot is beside the map on the outer side");
  const small = placed("right", 1280, 720);
  assert.deepEqual([small.mode, small.plan, small.enemy !== null], ["wide", "both", true]);
});

test("the free slot never shrinks the map; when space runs out our team stays complete", () => {
  const withSlot = placed("right", 1920, 1080);
  const sameHeightNoSlot = secondMonitorLayout({ side: "right", width: withSlot.map.w + 8 + 2 * ROW_CARD_MIN.w + 8, height: box(1920, 1080).height });
  assert.ok(sameHeightNoSlot.mode === "wide");
  assert.equal((sameHeightNoSlot as PlacedLayout).map.w, withSlot.map.w);
  const minimum = placed("right", 688, 680);
  assert.equal(minimum.plan, "ours");
  assert.equal(minimum.enemy, null);
});

test("below the smallest supported size the page says the window is too small; off stays off", () => {
  for (const [w, h] of [[680, 680], [688, 676], [500, 900], [400, 400]] as const) {
    assert.equal(secondMonitorLayout({ side: "right", ...box(w, h) }).mode, "too_small", `${w}x${h}`);
  }
  assert.equal(secondMonitorLayout({ side: "right", width: Number.NaN, height: 900 }).mode, "too_small");
  assert.equal(secondMonitorLayout({ side: "off", width: 1832, height: 1052 }).mode, "off");
});

test("the layout depends only on the box size, never on content", () => {
  assert.deepEqual(secondMonitorLayout({ side: "left", width: 1832, height: 1052 }), secondMonitorLayout({ side: "left", width: 1832, height: 1052 }));
});

test("the view places every region from the layout and renders the too-small state", () => {
  const deck = readFileSync(new URL("../src/live/SecondMonitorDeck.tsx", import.meta.url), "utf8");
  for (const region of ["map", "status", "ours", "enemy", "slot"]) assert.match(deck, new RegExp(`data-region="${region}"`));
  assert.match(deck, /layout\.mode === "too_small"/);
  assert.match(deck, /orderBySlot\(\[ownCard, \.\.\.allies\]\)/, "our cards ordered by slot id");
  assert.match(deck, /<details className="sm-own-more">/, "own details open in a popover, not by growing the card");
  assert.doesNotMatch(deck, /legacy/, "no fallback to the scrolling centred deck");
});
