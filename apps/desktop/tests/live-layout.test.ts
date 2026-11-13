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
  assert.match(canvas, /aspect-ratio:\s*280\s*\/\s*274/);
  assert.match(css, /\.live-preview-rates\s*\{[^}]*white-space:\s*nowrap[^}]*text-overflow:\s*ellipsis/);
  const autoRules = [...css.matchAll(/([^{}]+)\{[^{}]*overflow(?:-y)?:\s*auto[^{}]*\}/g)];
  assert.ok(autoRules.length > 0);
  for (const rule of autoRules) assert.match(rule[1]!, /live-disclosure-body/);
});

test("canvas box scales up or down to fit while preserving the frame header aspect", () => {
  assert.deepEqual(fitCanvasBox(960, 572, { width: 292, height: 292 }), { width: 572, height: 572 });
  const large = fitCanvasBox(1514, 884, { width: 280, height: 274 });
  assert.ok(large);
  assert.ok(large.width >= 700 && large.height >= 680);
  assert.ok(Math.abs(large.width / large.height - 280 / 274) < 1e-10);
  assert.deepEqual(fitCanvasBox(100, 50, { width: 200, height: 100 }), { width: 100, height: 50 });
  const fallback = fitCanvasBox(560, 548);
  assert.deepEqual(fallback, { width: 560, height: 548 });
});

test("unknown or empty stage dimensions do not invent a canvas box", () => {
  assert.equal(fitCanvasBox(0, 100), null);
  assert.equal(fitCanvasBox(Number.NaN, 100), null);
  assert.equal(fitCanvasBox(100, 100, { width: 0, height: 292 }), null);
});
