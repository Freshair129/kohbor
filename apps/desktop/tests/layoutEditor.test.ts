import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";
import {
  clampObjectiveWidget,
  OBJECTIVE_CARD_LAYOUT_DEFAULT,
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

test("default card stays in the 5 to 8 percent HUD band and 8 px inside both reference edges", () => {
  for (const viewport of [{ width: 1920, height: 1080 }, { width: 2560, height: 1440 }]) {
    const value = clampObjectiveWidget(OBJECTIVE_CARD_LAYOUT_DEFAULT, viewport);
    const width = 272 * viewport.width / 1920;
    const height = 32 * viewport.width / 1920;
    assert.ok(value.x_percent * viewport.width / 100 - width / 2 >= 8);
    assert.ok(value.x_percent * viewport.width / 100 + width / 2 <= viewport.width - 8);
    assert.ok(value.y_percent * viewport.height / 100 - height / 2 >= viewport.height * 0.05);
    assert.ok(value.y_percent * viewport.height / 100 + height / 2 <= viewport.height * 0.08);
  }
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
