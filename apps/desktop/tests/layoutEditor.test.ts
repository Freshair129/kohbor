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
  blockingReasons,
  clampObjectiveWidgetWithReasons,
  layoutReadoutPx,
  nudgeObjectiveWidget,
  parseLayoutDocument,
  pointerToLayout,
  referenceBackground,
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

test("editor v2: a pointer at the same screen point gives the same clamped layout inline and in 1:1 full screen", () => {
  const fullscreen = { left: 0, top: 0, width: 1920, height: 1080 };
  const inline = { left: 300, top: 120, width: 640, height: 360 };
  for (const [sx, sy] of [[960, 85], [200, 900], [960, 150], [5, 5], [1900, 1070]] as const) {
    const big = pointerToLayout(sx, sy, 0, 0, fullscreen, 0.75);
    const small = pointerToLayout(inline.left + sx / 3, inline.top + sy / 3, 0, 0, inline, 0.75);
    assert.ok(Math.abs(big.x_percent - small.x_percent) < 1e-9 && Math.abs(big.y_percent - small.y_percent) < 1e-9);
    const fromBig = clampObjectiveWidgetWithReasons(big);
    const fromSmall = clampObjectiveWidgetWithReasons(small);
    assert.ok(Math.abs(fromBig.layout.x_percent - fromSmall.layout.x_percent) < 1e-9);
    assert.ok(Math.abs(fromBig.layout.y_percent - fromSmall.layout.y_percent) < 1e-9);
    assert.deepEqual(fromBig.blockedBy, fromSmall.blockedBy);
  }
  const free = clampObjectiveWidgetWithReasons(pointerToLayout(960, 500, 0, 0, fullscreen, 0.75));
  assert.deepEqual(free.blockedBy, [], "free drag in open space is not snapped or blocked");
  assert.ok(Math.abs(free.layout.y_percent - 500 / 10.8) < 1e-9, "pixel-level position, no grid snap");
});

test("editor v2: the clamp names what stopped the card, including the 24 px announcement allowance", () => {
  const minimap = clampObjectiveWidgetWithReasons({ x_percent: 10, y_percent: 90, scale: 1 });
  assert.ok(minimap.blockedBy.includes("minimap"));
  const deep = clampObjectiveWidgetWithReasons({ x_percent: 50, y_percent: 12, scale: 1 });
  assert.deepEqual(deep.blockedBy, ["announcement"]);
  assert.ok(Math.abs(deep.layout.y_percent * 10.8 + 16 - (86.4 + 24)) < 1e-6);
  assert.deepEqual(blockingReasons({ x_percent: 50, y_percent: 8.6, scale: 0.75 }), [], "inside the allowance is not blocked");
  assert.ok(blockingReasons({ x_percent: 50, y_percent: 8.6, scale: 0.75 }, undefined, FULLY_BLOCKED).includes("announcement"));
  assert.ok(clampObjectiveWidgetWithReasons({ x_percent: -5, y_percent: 50, scale: 1 }).blockedBy.includes("edge"));
});

test("editor v2: arrow keys nudge 1 px and Shift+arrow 10 px at 1920x1080, still clamped", () => {
  const start = { x_percent: 50, y_percent: 50, scale: 1 };
  const right = nudgeObjectiveWidget(start, "ArrowRight", false)!;
  assert.equal(layoutReadoutPx(right.layout).x - layoutReadoutPx(start).x, 1);
  const down10 = nudgeObjectiveWidget(start, "ArrowDown", true)!;
  assert.equal(layoutReadoutPx(down10.layout).y - layoutReadoutPx(start).y, 10);
  assert.equal(nudgeObjectiveWidget(start, "Enter", false), null);
  const atTop = { ...OBJECTIVE_CARD_LAYOUT_DEFAULT, x_percent: 90, y_percent: (54 + 12) / 10.8 };
  const up = nudgeObjectiveWidget(atTop, "ArrowUp", true)!;
  assert.ok(Math.abs(up.layout.y_percent - atTop.y_percent) < 1e-9, "the top bar stops the nudge");
  assert.ok(up.blockedBy.includes("top_bar"));
});

test("editor v2: without a local reference image the editor falls back to the grid and shows how to add one", () => {
  assert.equal(referenceBackground(null), undefined);
  assert.equal(referenceBackground("https://example.invalid/x.jpg"), undefined, "only the host's JPEG data URL is accepted");
  assert.deepEqual(referenceBackground("data:image/jpeg;base64,AAAA"), { backgroundImage: 'url("data:image/jpeg;base64,AAAA")' });
  const editor = readFileSync(new URL("../src/layoutEditor/LayoutEditor.tsx", import.meta.url), "utf8");
  assert.match(editor, /\{!reference && <p className="layout-editor-hint" role="note">\{copy\.layoutNoReference\}<\/p>\}/);
  assert.match(editor, /style=\{referenceBackground\(reference\)\}/);
  assert.doesNotMatch(editor, /from "\.\.\/lab\//, "the Settings editor carries no Lab code into the normal bundle");
  assert.match(OBJECTIVE_COPY.th.layoutNoReference, /dota-overlay-reference\.jpg/);
  assert.match(OBJECTIVE_COPY.en.layoutNoReference, /dota-overlay-reference\.jpg/);
  const runtime = readFileSync(new URL("../src/runtime.ts", import.meta.url), "utf8");
  assert.match(runtime, /export async function layoutReferenceImage\(\)[\s\S]*?invoke<unknown>\("lab_reference_image"\)[\s\S]*?catch \{\s*return null;/);
});

test("editor v2: full screen, Esc and the shared Lab full-screen helper", () => {
  const editor = readFileSync(new URL("../src/layoutEditor/LayoutEditor.tsx", import.meta.url), "utf8");
  assert.match(editor, /event\.key === "Escape"\) leaveFullscreen\(\)/);
  assert.match(editor, /layout-editor-stage\$\{fullscreen \? " is-fullscreen" : ""\}/);
  const lab = readFileSync(new URL("../src/lab/HudLayoutEditor.tsx", import.meta.url), "utf8");
  assert.match(lab, /toggleElementFullscreen\(target\)/, "the Lab editor uses the same full-screen helper");
  const css = readFileSync(new URL("../src/styles.css", import.meta.url), "utf8");
  assert.match(css, /\.layout-editor-stage\.is-fullscreen \.layout-editor-preview \{ width: min\(100vw, calc\(100vh \* 16 \/ 9\)\)/);
  assert.match(css, /\.layout-protected\.is-blocking \{/);
});
