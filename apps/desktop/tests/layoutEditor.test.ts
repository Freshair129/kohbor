import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";
import {
  clampObjectiveWidget,
  FULLY_BLOCKED,
  GMETER_BASE_HEIGHT_PX,
  GMETER_DEFAULT_ANCHOR,
  OBJECTIVE_CARD_LAYOUT_DEFAULT,
  OBJECTIVE_CARD_RULES,
  parseLayoutDocument,
} from "../src/layoutEditor/model.ts";
import { OBJECTIVE_COPY } from "../src/copy.ts";

const cases = JSON.parse(readFileSync(new URL("./fixtures/layout-clamp-cases.json", import.meta.url), "utf8")) as Array<{
  name: string;
  viewport: { width: number; height: number };
  request: { x_percent: number; y_percent: number; scale: number };
  expected: { x_percent: number; y_percent: number; scale: number };
}>;

test("layout editor clamp matches the shared Rust cases for each protected region and edge", () => {
  for (const item of cases) {
    const actual = clampObjectiveWidget(item.request, item.viewport);
    assert.ok(Math.abs(actual.x_percent - item.expected.x_percent) < 1e-9, `${item.name} x: ${actual.x_percent} != ${item.expected.x_percent}`);
    assert.ok(Math.abs(actual.y_percent - item.expected.y_percent) < 1e-9, `${item.name} y: ${actual.y_percent} != ${item.expected.y_percent}`);
    assert.equal(actual.scale, item.expected.scale, `${item.name} scale`);
  }
});

test("default card sits centred under the RISK pill with a 4 px gap and stays compact (owner 2026-11-22)", () => {
  for (const viewport of [{ width: 1920, height: 1080 }, { width: 2560, height: 1440 }]) {
    const value = clampObjectiveWidget(OBJECTIVE_CARD_LAYOUT_DEFAULT, viewport);
    assert.deepEqual(value, OBJECTIVE_CARD_LAYOUT_DEFAULT, "the default needs no clamping");
    const px = viewport.width / 1920;
    const pillBottom = GMETER_DEFAULT_ANCHOR.y_percent * viewport.height / 100 + GMETER_BASE_HEIGHT_PX * GMETER_DEFAULT_ANCHOR.scale * px / 2;
    const cardHalf = 16 * value.scale * px;
    const cardTop = value.y_percent * viewport.height / 100 - cardHalf;
    const cardBottom = value.y_percent * viewport.height / 100 + cardHalf;
    assert.equal(value.x_percent, GMETER_DEFAULT_ANCHOR.x_percent);
    assert.ok(Math.abs(cardTop - pillBottom - 4 * px) < 1e-6, "4 px gap under RISK");
    assert.ok(232 * value.scale <= 170 * GMETER_DEFAULT_ANCHOR.scale * 1.6, "at most 1.6x the RISK pill width");
    assert.ok(cardTop >= viewport.height * 0.05);
    assert.ok(cardBottom <= viewport.height * 0.08 + 24 * viewport.height / 1080);
  }
});

test("only the objective card may enter the top 24 px (at 1080) of the announcement region", () => {
  const deep = clampObjectiveWidget({ x_percent: 50, y_percent: 12, scale: 1 }, { width: 1920, height: 1080 });
  assert.ok(Math.abs(deep.y_percent * 10.8 + 16 - (86.4 + 24)) < 1e-6, "dragging deeper stops at the allowance");
  const deep1440 = clampObjectiveWidget({ x_percent: 50, y_percent: 12, scale: 1 }, { width: 2560, height: 1440 });
  assert.ok(Math.abs(deep1440.y_percent * 14.4 + 16 * 2560 / 1920 - (1440 * 0.08 + 32)) < 1e-6, "scaled with screen height");
  const blocked = clampObjectiveWidget(OBJECTIVE_CARD_LAYOUT_DEFAULT, { width: 1920, height: 1080 }, FULLY_BLOCKED);
  assert.ok(blocked.y_percent * 10.8 + 16 * blocked.scale <= 86.4 + 1e-9, "widgets without the allowance stay fully blocked");
  assert.notDeepEqual(FULLY_BLOCKED, OBJECTIVE_CARD_RULES);
});

test("a saved revision 4 default reads as the new default and a user layout still wins", () => {
  const legacy = parseLayoutDocument({ schema_version: 1, widgets: { objective_card: { x_percent: 62.5, y_percent: 6.5, scale: 1 } } });
  assert.deepEqual(legacy?.widgets.objective_card, OBJECTIVE_CARD_LAYOUT_DEFAULT);
  const moved = parseLayoutDocument({ schema_version: 1, widgets: { objective_card: { x_percent: 20, y_percent: 40, scale: 1 } } });
  assert.equal(moved?.widgets.objective_card.x_percent, 20);
});

test("layout document accepts only the registered objective card and strict fields", () => {
  const valid = { schema_version: 1, widgets: { objective_card: { ...OBJECTIVE_CARD_LAYOUT_DEFAULT } } };
  assert.deepEqual(parseLayoutDocument(valid), valid);
  assert.equal(parseLayoutDocument({ ...valid, widgets: { ...valid.widgets, risk_bar: { x_percent: 20, y_percent: 20, scale: 1 } } }), null);
  assert.equal(parseLayoutDocument({ schema_version: 1, widgets: { objective_card: { ...OBJECTIVE_CARD_LAYOUT_DEFAULT, extra: 0 } } }), null);
});

test("layout editor copy gives bilingual drag, size, and reset instructions", () => {
  assert.match(OBJECTIVE_COPY.th.layoutHelp, /ลากการ์ด/u);
  assert.match(OBJECTIVE_COPY.en.layoutHelp, /Drag the card/u);
  assert.match(OBJECTIVE_COPY.th.layoutReset, /คืนค่า/u);
  assert.match(OBJECTIVE_COPY.en.layoutReset, /Reset/u);
});
