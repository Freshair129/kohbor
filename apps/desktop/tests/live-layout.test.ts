import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { test } from "node:test";

const { fitCanvasBox } = await import("../src/live/canvasBox.ts");
const { centeredLiveLayout, ownFieldCardMode } = await import("../src/live/secondMonitorLayout.ts");

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
  // Owner report on build 4467634 (2026-11-24): sent/received/painted rates are shown once, in the footer (CAP/RX/PAINT).
  const panel = readFileSync(new URL("../src/live/MinimapPanel.tsx", import.meta.url), "utf8");
  assert.doesNotMatch(panel, /live-preview-rates|previewRateText/);
  assert.match(panel, /<ScoreboardHeader snapshot=\{snapshot\} phase=\{gamePhase\}/);
  assert.doesNotMatch(panel, /live-map-badge|live-map-plate/);
  assert.match(panel, /className="live-map-body is-field"/);
  assert.match(panel, /<div className="live-map-square-slot">\{stage\}<\/div>/);
  assert.doesNotMatch(css, /\.live-preview-rates/);
  const autoRules = [...css.matchAll(/([^{}]+)\{[^{}]*overflow(?:-y)?:\s*auto[^{}]*\}/g)];
  for (const rule of autoRules) assert.doesNotMatch(rule[1]!, /\.live-tab/, "live panels do not scroll themselves; the dialog does");
});

test("centred map controls stay in one compact header above the largest square stage", () => {
  const liveCss = readFileSync(new URL("../src/live/live.css", import.meta.url), "utf8").replace(/\/\*[\s\S]*?\*\//g, "");
  const deckCss = readFileSync(new URL("../src/live/secondMonitor.css", import.meta.url), "utf8").replace(/\/\*[\s\S]*?\*\//g, "");
  const toolbar = deckCss.match(/\.deck-field-map \.live-map-toolbar\s*\{([^}]+)\}/)?.[1] ?? "";
  const pane = deckCss.match(/\.deck-field-map > \.live-map-pane\s*\{([^}]+)\}/)?.[1] ?? "";
  const body = liveCss.match(/\.live-map-body\.is-field\s*\{([^}]+)\}/)?.[1] ?? "";
  const slot = liveCss.match(/\.live-map-body\.is-field > \.live-map-square-slot\s*\{([^}]+)\}/)?.[1] ?? "";
  const square = liveCss.match(/\.live-map-body\.is-field > \.live-map-square-slot > \.live-map-stage\s*\{([^}]+)\}/)?.[1] ?? "";
  assert.match(toolbar, /flex-wrap:\s*nowrap/);
  assert.match(toolbar, /flex:\s*0\s+0\s+54px/);
  assert.match(toolbar, /height:\s*54px/);
  assert.match(toolbar, /align-items:\s*center/);
  assert.match(pane, /container-type:\s*size/);
  assert.match(pane, /gap:\s*0/);
  assert.match(body, /flex-direction:\s*column/);
  assert.match(body, /height:\s*min\(100cqw,\s*100cqh\)/);
  assert.match(slot, /width:\s*min\(100cqw,\s*100cqh\)/);
  assert.match(slot, /height:\s*min\(100cqw,\s*100cqh\)/);
  assert.match(square, /width:\s*100%/);
  assert.match(square, /height:\s*100%/);
  assert.match(square, /aspect-ratio:\s*1/);
});

test("roomy centred cards use replay-sized icons and separated header rows above their measured floor", () => {
  const cardCss = readFileSync(new URL("../src/live/heroCard.css", import.meta.url), "utf8").replace(/\/\*[\s\S]*?\*\//g, "");
  const cardSource = readFileSync(new URL("../src/live/HeroCard.tsx", import.meta.url), "utf8");
  const roomy = cardCss.match(/\.lhc-field-card\.is-roomy-field\s*\{([^}]+)\}/)?.[1] ?? "";
  assert.match(roomy, /grid-template-rows:\s*28px\s+14px\s+25px\s+30px\s+30px\s+minmax\(36px,\s*1fr\)/);
  assert.match(cardCss, /\.lhc-field-card\.is-roomy-field \.lhc-ic\.is-ab\s*\{[^}]*width:\s*28px;\s*height:\s*28px/);
  assert.match(cardCss, /\.lhc-field-card\.is-roomy-field \.lhc-ic\.is-it\s*\{[^}]*width:\s*28px;\s*height:\s*20px/);
  assert.match(cardCss, /\.lhc-field-card\.is-roomy-field \.lhc-stats\s*\{[^}]*repeat\(5,\s*minmax\(0,\s*1fr\)\)[^}]*repeat\(3,\s*minmax\(12px,\s*1fr\)\)/);
  assert.match(cardSource, /roomyField && <div className="lhc-econ"/);
  assert.doesNotMatch(cardSource, /lhc-derived-basis is-inline/);
});

test("centred own card keeps icons and all stat rows inside both target card sizes and anchors the map header at the top", () => {
  const cardCss = readFileSync(new URL("../src/live/heroCard.css", import.meta.url), "utf8").replace(/\/\*[\s\S]*?\*\//g, "");
  const deckCss = readFileSync(new URL("../src/live/secondMonitor.css", import.meta.url), "utf8").replace(/\/\*[\s\S]*?\*\//g, "");
  const liveCss = readFileSync(new URL("../src/live/live.css", import.meta.url), "utf8").replace(/\/\*[\s\S]*?\*\//g, "");
  const cardSource = readFileSync(new URL("../src/live/HeroCard.tsx", import.meta.url), "utf8");
  const panelSource = readFileSync(new URL("../src/live/MinimapPanel.tsx", import.meta.url), "utf8");
  const cssRule = (css: string, selector: string) => {
    const escaped = selector.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
    const matches = [...css.matchAll(new RegExp(`${escaped}\\s*\\{([^}]+)\\}`, "g"))];
    return matches.at(-1)?.[1] ?? "";
  };
  const minHeight = (body: string, fallbackGap: number) => {
    const rowText = body.match(/grid-template-rows:\s*([^;]+)/)?.[1];
    assert.ok(rowText, "the card declares every row track");
    const rows = [...rowText.matchAll(/minmax\((\d+)px|(?<!minmax\()\b(\d+)px/g)].map(match => Number(match[1] ?? match[2]));
    const gap = Number(body.match(/(?:^|;)\s*gap:\s*(\d+)(?:px)?/)?.[1] ?? fallbackGap);
    const paddingText = body.match(/padding-block:\s*(\d+)px|padding:\s*(\d+)px(?:\s|;)/);
    const padding = Number(paddingText?.[1] ?? paddingText?.[2] ?? 0);
    return rows.reduce((sum, row) => sum + row, 0) + (rows.length - 1) * gap + 2 * padding + 2;
  };
  const ownBase = cssRule(cardCss, ".lhc-field-card.lhc-field-own");
  const ownWide = cssRule(cardCss, ".lhc-field-card.lhc-field-own.has-wide-stats");
  const roomyBase = cssRule(cardCss, ".lhc-field-card.is-roomy-field");
  const ownRow28 = cssRule(cardCss, ".lhc-field-card.lhc-field-own.is-own-one-row-28");
  const ownRow22 = cssRule(cardCss, ".lhc-field-card.lhc-field-own.is-own-two-row-22");
  const ownStatsRule = cardCss.match(/\.lhc-field-card\.lhc-field-own\.is-own-one-row-28 \.lhc-stats,[\s\S]*?\{([^}]+)\}/)?.[1] ?? "";
  const compactTeam = cssRule(cardCss, ".lhc-field-card.is-compact-fit");
  assert.ok(cardSource.includes("className={`lhc-row lhc-ab") && cardSource.includes("className={`lhc-row lhc-it")
    && cardSource.includes('className="lhc-stat-stack"'),
    "abilities, items, and stat rows are rendered in the shared own-card section");
  assert.match(ownRow28, /grid-template-areas:\s*"pt nm top"\s+"bars bars bars"\s+"ab ab it"\s+"st st st"/);
  assert.match(ownRow28, /grid-template-rows:\s*24px\s+24px\s+28px\s+minmax\(36px,/);
  assert.match(cardCss, /\.lhc-field-card\.lhc-field-own\.is-own-one-row-28 \.lhc-ic\.is-ab\s*\{[^}]*height:\s*28px/);
  assert.match(cardCss, /\.lhc-field-card\.lhc-field-own\.is-own-one-row-28 \.lhc-ic\.is-it\s*\{[^}]*height:\s*20px/);
  assert.match(ownRow22, /grid-template-rows:\s*16px\s+24px\s+18px\s+18px\s+minmax\(36px,/);
  assert.match(ownRow22, /grid-template-areas:\s*"pt nm top"\s+"bars bars bars"\s+"ab ab ab"\s+"it it it"\s+"st st st"/,
    "the minimum viewport keeps separate ability, item, and stat tracks inside the own card");
  assert.match(cardCss, /\.lhc-field-card\.lhc-field-own\.is-own-two-row-22 \.lhc-ic\.is-ab\s*\{[^}]*height:\s*16px/);
  assert.match(cardCss, /\.lhc-field-card\.lhc-field-own\.is-own-two-row-22 \.lhc-ic\.is-it\s*\{[^}]*height:\s*16px/);
  assert.match(ownStatsRule, /grid-template-columns:\s*repeat\(5,/);
  assert.match(ownStatsRule, /grid-template-rows:\s*repeat\(3,\s*minmax\(12px,/);
  assert.match(cardCss, /\.lhc-field-card\.lhc-field-own\.is-own-two-row-22 \.lhc-side\s*\{[^}]*gap:\s*0/,
    "the 22 px own-card item row fits inside the minimum card content width");
  assert.match(compactTeam, /grid-template-rows:\s*16px\s+14px\s+22px\s+22px\s+minmax\(36px,/);
  assert.match(compactTeam, /"ab ab ab"\s+"it it it"\s+"st st st"/);
  assert.match(cardCss, /\.lhc-field-card\.is-compact-fit \.lhc-stats\s*\{[^}]*repeat\(5,\s*minmax\(0,\s*1fr\)\)[^}]*repeat\(3,\s*minmax\(12px,/);
  assert.match(cardSource, /roomyField \? " is-roomy-field" : " is-compact-fit"/);
  assert.match(cardSource, /is-own-\$\{ownFieldMode\}/);
  assert.match(ownWide, /grid-template-rows:\s*16px\s+14px\s+14px\s+14px\s+minmax\(48px,/);

  const viewports = [[1920, 1080, "one-row-28"], [1280, 720, "two-row-22"]] as const;
  for (const [width, height, expectedMode] of viewports) {
    const layout = centeredLiveLayout({ width: width - 208, height: height - 132 });
    assert.equal(layout.mode, "field", `${width}x${height}: centered layout is available`);
    if (layout.mode !== "field") continue;
    const mode = ownFieldCardMode(layout.card);
    assert.equal(mode, expectedMode, `${width}x${height}: own card chooses the correct icon layout`);
    const style = mode === "one-row-28" ? ownRow28 : ownRow22;
    const floor = minHeight(style, Number(ownBase.match(/gap:\s*(\d+)px/)?.[1] ?? 1));
    assert.ok(layout.card.h >= floor, `${width}x${height}: own card ${layout.card.h}px contains its ${floor}px row minimum`);
    if (width === 1280) assert.ok(layout.card.h >= minHeight(compactTeam, 0), "the nine unknown cards keep three readable stat lines at the minimum viewport");
  }

  const pane = cssRule(deckCss, ".deck-field-map > .live-map-pane");
  const toolbar = cssRule(deckCss, ".deck-field-map .live-map-toolbar");
  const paneBase = cssRule(liveCss, ".live-map-pane");
  const padding = Number(pane.match(/padding:\s*(\d+)px/)?.[1] ?? NaN);
  const border = Number(paneBase.match(/border:\s*(\d+)px/)?.[1] ?? NaN);
  const fieldMarkup = panelSource.match(/\) : field \? \([\s\S]*?\) : \(/)?.[0] ?? "";
  assert.match(fieldMarkup, /\{toolbar\}\s*<div className="live-map-body is-field"/);
  assert.match(pane, /gap:\s*0/);
  assert.match(toolbar, /flex:\s*0\s+0\s+54px/);
  assert.match(toolbar, /height:\s*54px/);
  assert.match(toolbar, /align-items:\s*center/);
  assert.ok(Number.isFinite(padding + border) && padding + border <= 8, "the first header pixel starts within 8 px of the pane top");
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
