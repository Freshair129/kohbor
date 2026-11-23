import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { test } from "node:test";

const { fitCanvasBox } = await import("../src/live/canvasBox.ts");

test("Live has one definite grid row, two bounded columns and a shrinking map stage", () => {
  const css = readFileSync(new URL("../src/live/live.css", import.meta.url), "utf8");
  const root = css.match(/\.live-tab\s*\{([^}]+)\}/)?.[1] ?? "";
  assert.match(root, /display:\s*grid/);
  assert.match(root, /grid-template-columns:\s*minmax\(0,\s*1fr\)\s+minmax\(280px,\s*340px\)/);
  assert.match(root, /grid-template-rows:\s*minmax\(0,\s*1fr\)/);
  assert.match(root, /height:\s*100%/);
  assert.match(root, /min-height:\s*0/);
  assert.match(root, /overflow:\s*hidden/);
  assert.match(css, /main\.live-tab\.live-dashboard\s*\{[^}]*grid-template-rows:\s*minmax\(0,\s*1fr\)/);
  const stage = css.match(/\.live-map-stage\s*\{([^}]+)\}/)?.[1] ?? "";
  assert.match(stage, /flex:\s*1\s+1\s+0/);
  assert.match(stage, /min-height:\s*0/);
  assert.match(stage, /display:\s*grid/);
  assert.match(stage, /place-items:\s*center/);
  const canvas = css.match(/\.live-map-canvas\s*\{([^}]+)\}/)?.[1] ?? "";
  for (const property of ["max-width", "max-height"]) assert.match(canvas, new RegExp(`${property}:\\s*100%`));
  for (const property of ["width", "height"]) assert.match(canvas, new RegExp(`(?:^|;)\\s*${property}:\\s*auto`));
  assert.match(canvas, /aspect-ratio:\s*280\s*\/\s*280/);
  const toolbar = css.match(/\.live-map-toolbar\s*\{([^}]+)\}/)?.[1] ?? "";
  assert.match(toolbar, /flex:\s*0\s+0\s+auto/);
  assert.match(toolbar, /flex-wrap:\s*wrap/);
  assert.match(toolbar, /min-height:\s*0/);
  const deckCss = readFileSync(new URL("../src/deck/deck.css", import.meta.url), "utf8");
  const deckToolbar = deckCss.match(/\.live-deck-panel\s+\.live-map-toolbar\s*\{([^}]+)\}/)?.[1] ?? "";
  assert.doesNotMatch(deckToolbar, /flex(?:-basis)?\s*:\s*[^;]*(?:\d+(?:px|rem|em|%))/,
    "deck layout does not fix the height of a wrapping toolbar");
  const textRule = css.match(/\.live-map-badge\s*\{([^}]+)\}/)?.[1] ?? "";
  assert.match(textRule, /overflow-wrap:\s*anywhere/);
  assert.match(textRule, /white-space:\s*normal/);
  assert.doesNotMatch(textRule, /overflow:\s*hidden|text-overflow:\s*ellipsis|white-space:\s*nowrap|max-width:\s*\d/);
  // Owner report on build 4467634 (2026-11-24): sent/received/painted rates are shown once, in the footer (CAP/RX/PAINT).
  const panel = readFileSync(new URL("../src/live/MinimapPanel.tsx", import.meta.url), "utf8");
  assert.doesNotMatch(panel, /live-preview-rates|previewRateText/);
  assert.match(panel, /className="live-map-badge mono"/);
  assert.doesNotMatch(css, /\.live-preview-rates/);
  const autoRules = [...css.matchAll(/([^{}]+)\{[^{}]*overflow(?:-y)?:\s*auto[^{}]*\}/g)];
  for (const rule of autoRules) assert.doesNotMatch(rule[1]!, /\.live-tab/, "live panels do not scroll themselves; the dialog does");
});

test("canvas box scales up or down to fit while preserving the frame header aspect", () => {
  assert.deepEqual(fitCanvasBox(960, 572, { width: 292, height: 292 }), { width: 572, height: 572 });
  const large = fitCanvasBox(1514, 884, { width: 280, height: 274 });
  assert.ok(large);
  assert.ok(large.width >= 700 && large.height >= 680);
  assert.ok(Math.abs(large.width / large.height - 280 / 274) < 1e-10);
  assert.deepEqual(fitCanvasBox(100, 50, { width: 200, height: 100 }), { width: 100, height: 50 });
  const fallback = fitCanvasBox(560, 548);
  assert.deepEqual(fallback, { width: 548, height: 548 });
});

test("unknown or empty stage dimensions do not invent a canvas box", () => {
  assert.equal(fitCanvasBox(0, 100), null);
  assert.equal(fitCanvasBox(Number.NaN, 100), null);
  assert.equal(fitCanvasBox(100, 100, { width: 0, height: 292 }), null);
});

test("diagnostic columns shrink within the existing bounded resource popover", () => {
  const css = readFileSync(new URL("../src/live/live.css", import.meta.url), "utf8");
  const grid = css.match(/\.live-preview-diagnostic-grid\s*\{([^}]+)\}/)?.[1] ?? "";
  assert.match(grid, /grid-template-columns:\s*repeat\(2,\s*minmax\(0,\s*1fr\)\)/);
  assert.match(grid, /min-width:\s*0/);
  assert.doesNotMatch(css, /\.live-disclosure-body/, "no floating absolute popover remains");
  const dialogCss = readFileSync(new URL("../src/shared/dialog.css", import.meta.url), "utf8");
  assert.match(dialogCss.match(/\.dialog-body\s*\{([^}]+)\}/)?.[1] ?? "", /overflow:\s*auto/);
  assert.match(dialogCss.match(/\.dialog-panel\s*\{([^}]+)\}/)?.[1] ?? "", /max-height:\s*100%/);
});
