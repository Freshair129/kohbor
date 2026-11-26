import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { test } from "node:test";
// @ts-ignore Node's built-in TypeScript loader requires the explicit source extension.
import { outerPanelBox, pinnedPanelLayout, PANEL_MIN_HEIGHT, PANEL_PRIORITY } from "../src/live/pinnedPanelsModel.ts";
// @ts-ignore Node's built-in TypeScript loader requires the explicit source extension.
import { secondMonitorLayout } from "../src/live/secondMonitorLayout.ts";

const EXPECTED: Record<string, readonly string[]> = {
  "1920x1080": ["objectives", "damage", "orb", "farm", "kills"],
  // A maximized window on a 1920x1080 screen loses the title bar and taskbar; all five must still fit (UI-016).
  "1920x1032": ["objectives", "damage", "orb", "farm", "kills"],
  "1920x1001": ["objectives", "damage", "orb", "farm", "kills"],
  "1600x900": ["objectives", "damage", "orb", "farm"],
  "1440x900": ["objectives", "damage"],
};
const panelCss = readFileSync(new URL("../src/live/secondMonitor.css", import.meta.url), "utf8");

function boxFor(width: number, height: number, side: "left" | "right") {
  // The control viewport reserves the measured app rails/header/footer before the responsive deck is placed.
  const hostWidth = width - 72;
  const hostHeight = height - 178;
  const layout = secondMonitorLayout({ side, width: hostWidth, height: hostHeight });
  assert.notEqual(layout.mode, "too_small");
  assert.notEqual(layout.mode, "off");
  if (layout.mode === "off" || layout.mode === "too_small") throw new Error("unexpected base layout");
  return { layout, outer: outerPanelBox(layout) };
}

for (const [width, height] of [[1920, 1080], [1920, 1032], [1920, 1001], [1600, 900], [1440, 900]] as const) {
  for (const side of ["left", "right"] as const) {
    test(`pinned outer panels at ${width}x${height} on the ${side} edge fit without overlap or scrolling`, () => {
      const { layout, outer } = boxFor(width, height, side);
      const result = pinnedPanelLayout(outer);
      const expected = EXPECTED[`${width}x${height}`];
      assert.deepEqual(result.panels.map(panel => panel.id), expected);
      assert.deepEqual(result.dropped, PANEL_PRIORITY.slice(expected.length));
      assert.equal(result.minimumTextPx, 12);
      assert.equal(result.scroll, false);
      assert.match(panelCss, /\.pp-panel\s*\{[^}]*overflow:\s*hidden/);
      assert.doesNotMatch(panelCss, /\.pp-[^{]+\{[^}]*overflow-y:\s*auto/);
      for (const match of panelCss.matchAll(/([^{}]+)\{([^{}]*font-size:\s*(\d+(?:\.\d+)?)px[^{}]*)\}/g)) {
        if (/\.pp-|\.sm-roshan-status/.test(match[1])) assert.ok(Number(match[3]) >= 12, `${match[1].trim()} has font-size ${match[3]}px`);
      }
      for (const panel of result.panels) {
        assert.ok(panel.rect.x >= 0 && panel.rect.y >= 0);
        assert.ok(panel.rect.x + panel.rect.w <= outer.w);
        assert.ok(panel.rect.y + panel.rect.h <= outer.h);
      }
      for (let left = 0; left < result.panels.length; left++) {
        for (let right = left + 1; right < result.panels.length; right++) {
          const a = result.panels[left].rect, b = result.panels[right].rect;
          const overlaps = a.x < b.x + b.w && a.x + a.w > b.x && a.y < b.y + b.h && a.y + a.h > b.y;
          assert.equal(overlaps, false, `${result.panels[left].id} overlaps ${result.panels[right].id}`);
        }
      }
      for (const panel of result.panels) assert.ok(panel.rect.h >= PANEL_MIN_HEIGHT[panel.id], `${panel.id} is below its legible height`);
      // Every column is filled to the bottom of the box: no ragged rows and no empty strip.
      for (const x of new Set(result.panels.map(panel => panel.rect.x))) {
        const column = result.panels.filter(panel => panel.rect.x === x).sort((a, b) => a.rect.y - b.rect.y);
        assert.equal(column[0].rect.y, 0);
        assert.equal(column[column.length - 1].rect.y + column[column.length - 1].rect.h, outer.h);
        for (let index = 1; index < column.length; index++) {
          assert.equal(column[index].rect.y, column[index - 1].rect.y + column[index - 1].rect.h + 12);
        }
      }
      assert.ok(outer.x >= 0 && outer.y >= layout.map.y);
    });
  }
}
