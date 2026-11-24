import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { test } from "node:test";
import {
  CARD_MIN_W,
  CARD_ROOMY_ROW_MIN_H,
  CARD_GAP,
  CARD_ROW_MIN_H,
  CENTER_CARD_GAP,
  CENTER_CARD_MIN,
  CENTER_CARD_NARROW_STATS_MIN_H,
  CENTER_GAP,
  CENTER_MAP_MIN_W,
  CENTER_ROOMY_CARD_MIN_H,
  CENTER_WIDE_STATS_MIN_W,
  FIELD_TWO_ROW_24_MIN_W,
  GAP,
  MAP_PANEL_MIN_H,
  PINNED_DRAFT_MIN_MAP_H,
  PINNED_SCOREBOARD_HEIGHT,
  centeredLiveLayout,
  ownFieldCardMode,
  pinnedDraftFits,
  pinnedMapContents,
  placedRegions,
  secondMonitorLayout,
  type PlacedLayout,
  type Rect,
} from "../src/live/secondMonitorLayout.ts";
import * as heroCardModel from "../src/live/heroCardModel.ts";

// At the minimum viewport the pinned Live box retains the global header/footer and one navigation rail.
const liveBox = (w: number, h: number) => ({ width: w - 136, height: h - 132 });
const WINDOWS: [string, number, number][] = [
  ["1920x1080", 1920, 1080], ["1920x1000", 1920, 1000], ["1600x900", 1600, 900],
  ["1366x768", 1366, 768], ["1280x720", 1280, 720],
];

function placed(side: "left" | "right", w: number, h: number): PlacedLayout {
  const layout = secondMonitorLayout({ side, ...liveBox(w, h) });
  assert.equal(layout.mode, "stacked", `${w}x${h} ${side}: expected the pinned three-row layout`);
  return layout as PlacedLayout;
}
const inside = (r: Rect, w: number, h: number) => r.x >= 0 && r.y >= 0 && r.x + r.w <= w && r.y + r.h <= h && r.w > 0 && r.h > 0;
const overlap = (a: Rect, b: Rect) => Math.min(a.x + a.w, b.x + b.w) - Math.max(a.x, b.x) > 0 && Math.min(a.y + a.h, b.y + b.h) - Math.max(a.y, b.y) > 0;

test("pinned full replay-structure floors fit five cards per row at 12 px text", () => {
  assert.equal(CARD_MIN_W, 224);
  assert.equal(CARD_ROW_MIN_H, 156);
  assert.equal(CARD_ROOMY_ROW_MIN_H, 157);
  assert.equal(MAP_PANEL_MIN_H, 260);
  const css = readFileSync(new URL("../src/live/heroCard.css", import.meta.url), "utf8").replace(/\/\*[\s\S]*?\*\//g, "");
  assert.match(css, /\.lhc-field-card\.lhc-field-own\s*\{[^}]*minmax\(86px,\s*1fr\)/, "the legacy narrow own-card rule remains the fallback below the compact threshold");
  assert.match(css, /\.lhc-field-card\.lhc-field-own\.has-wide-stats\s*\{[^}]*minmax\(48px,\s*1fr\)/, "the base wide own-card rule remains available outside the explicit compact field modes");
});

test("both pinned sides fit ten complete card slots and the minimap at every requested viewport", () => {
  for (const [name, w, h] of WINDOWS) for (const side of ["left", "right"] as const) {
    const layout = placed(side, w, h);
    const { width, height } = liveBox(w, h);
    const regions = placedRegions(layout);
    for (const [region, rect] of regions) assert.ok(inside(rect, width, height), `${name} ${side}: ${region} outside ${JSON.stringify(rect)}`);
    for (let i = 0; i < regions.length; i++) for (let j = i + 1; j < regions.length; j++) {
      assert.ok(!overlap(regions[i][1], regions[j][1]), `${name} ${side}: ${regions[i][0]} overlaps ${regions[j][0]}`);
    }
    assert.equal(layout.map.x, 0, `${name}: full-width map panel reaches the inner edge`);
    assert.equal(layout.map.w, width, `${name}: each card row and the map panel share width`);
    assert.ok(layout.map.h >= MAP_PANEL_MIN_H, `${name}: map panel floor`);
    assert.ok(layout.card.w >= CARD_MIN_W, `${name}: each of five cards has at least ${CARD_MIN_W}px`);
    assert.ok(layout.card.h >= CARD_ROW_MIN_H, `${name}: each card row has at least ${CARD_ROW_MIN_H}px`);
    const roomy = layout.card.w >= 320 && layout.card.h >= CARD_ROOMY_ROW_MIN_H;
    assert.equal(roomy, name === "1920x1080" || name === "1920x1000", `${name}: expanded card rows use only the measured width and height`);
    const ownMode = ownFieldCardMode(layout.card);
    assert.equal(ownMode, layout.card.w >= 254 ? "two-row-24" : "two-row-22", `${name}: pinned own card preserves every icon row at the measured card width`);
    assert.equal(layout.enemy.y, 0, `${name}: enemy row starts at the top`);
    assert.equal(layout.enemy.h, layout.card.h);
    assert.equal(layout.map.y, layout.enemy.y + layout.enemy.h + GAP, `${name}: map follows the enemy row`);
    assert.equal(layout.ours.y, layout.map.y + layout.map.h + GAP, `${name}: our row follows the map`);
    assert.equal(layout.ours.h, layout.enemy.h);
    assert.equal(layout.card.w, Math.floor((width - 4 * CARD_GAP) / 5), `${name}: five equal card columns after their gaps`);
    const cardRects = [layout.enemy, layout.ours].flatMap(row => Array.from({ length: 5 }, (_, index) => ({
      x: row.x + index * (layout.card.w + CARD_GAP), y: row.y, w: layout.card.w, h: layout.card.h,
    })));
    assert.equal(cardRects.length, 10);
    cardRects.forEach((rect, index) => assert.ok(inside(rect, width, height), `${name} ${side}: card ${index} outside ${JSON.stringify(rect)}`));
    assert.equal(layout.mapEdge, side === "right" ? "left" : "right");
    assert.deepEqual(regions.map(([region]) => region), ["enemy", "map", "ours"]);
  }
});

test("pinned view mounts both five-card rows and has no free-slot or internal-scroll region", () => {
  const deck = readFileSync(new URL("../src/live/SecondMonitorDeck.tsx", import.meta.url), "utf8");
  const panel = readFileSync(new URL("../src/live/MinimapPanel.tsx", import.meta.url), "utf8");
  const css = readFileSync(new URL("../src/live/secondMonitor.css", import.meta.url), "utf8").replace(/\/\*[\s\S]*?\*\//g, "");
  const cardCss = readFileSync(new URL("../src/live/heroCard.css", import.meta.url), "utf8").replace(/\/\*[\s\S]*?\*\//g, "");
  assert.match(deck, /<TeamRow cards=\{enemies\} side="enemy"/);
  assert.match(deck, /<TeamRow cards=\{ours\} side="ally"/);
  assert.match(deck, /orderBySlot\(\[ownCard, \.\.\.allies\]\)/);
  assert.match(deck, /CompactHeroCards cards=\{cards\}[^>]*fieldCard/);
  assert.match(deck, /fieldCard fieldDirection="row"/);
  assert.match(deck, /<OwnHeroCard[^>]*fieldCard/);
  assert.match(deck, /placed\.card\.h >= CARD_ROOMY_ROW_MIN_H/);
  assert.match(deck, /layout="pinned" mapEdge=\{placed\.mapEdge\}/);
  assert.match(panel, /className=\{`live-map-body is-pinned\$\{stageOverride \? " is-draft" : ""\}`\} data-map-edge=\{mapEdge\}/);
  assert.match(panel, /<div className="live-map-pinned-stack">[\s\S]*?<div className=\{stageOverride \? "live-map-draft-slot" : "live-map-square-slot"\}>\{stage\}<\/div>/);
  assert.doesNotMatch(deck, /data-region="slot"|sm-slot|EventFeed|AnnouncerStatus|GsiSetupHint/);
  assert.doesNotMatch(css, /overflow(?:-[xy])?:\s*(?:auto|scroll)/, "pinned Live has no scroll containers");
  assert.match(css, /\.sm-host\s*\{[^}]*overflow:\s*hidden/);
  assert.match(css, /\.sm-card-row > \.lhc-list-field\s*\{[^}]*height:\s*100%/);
  assert.match(css, /\.sm-map-region > \.live-pinned-panel\s*\{[^}]*height:\s*100%/);
  assert.match(css, /\.live-map-body\.is-pinned\s*\{[^}]*--pinned-scoreboard-height:\s*64px/);
  assert.match(css, /\.live-map-pinned-stack\s*\{[^}]*grid-template-rows:\s*var\(--pinned-scoreboard-height\) minmax\(0,\s*1fr\)/);
  assert.match(css, /width:\s*min\(100cqw,\s*calc\(100cqh - var\(--pinned-scoreboard-height\)\)\)/,
    "the map uses the largest square after the scoreboard height");
  assert.match(css, /\.live-map-body\.is-pinned\.is-draft[^}]*\.live-map-draft-slot[^}]*width:\s*100%/,
    "the draft table receives the full map-region width");
  assert.match(css, /\.sm-board-hero[^}]*overflow:\s*visible[^}]*overflow-wrap:\s*anywhere[^}]*white-space:\s*normal/);
  assert.match(css, /\.sm-board-player[^}]*overflow:\s*visible[^}]*overflow-wrap:\s*anywhere[^}]*white-space:\s*normal/);
  assert.match(css, /@media\s*\(max-height:\s*940px\)[\s\S]*?\.sm-board-slot\s*\{[^}]*grid-template-columns:\s*clamp\(48px,\s*4vw,\s*64px\) minmax\(0,\s*1fr\)/);
  assert.match(cardCss, /\.lhc-field-card\.is-compact-fit \.lhc-side\s*\{[^}]*gap:\s*0/,
    "the 22 px item row fits the minimum pinned card track without horizontal clipping");
  assert.match(css, /.sm-host \.lhc-field-card\.is-roomy-field\s*\{[^}]*28px 12px 16px 28px 28px minmax\(36px,\s*1fr\)/,
    "pinned roomy rows preserve three 12 px stat lines inside their 160 px row cap");
});

test("pinned scoreboard, map slot, canvas, and draft board fit the 1920x1080 and 1440x900 map regions", () => {
  const windows: [string, number, number][] = [["1920x1080", 1920, 1080], ["1440x900", 1440, 900]];
  const content = readFileSync(new URL("../src/live/useCanvasBox.ts", import.meta.url), "utf8");
  const panel = readFileSync(new URL("../src/live/MinimapPanel.tsx", import.meta.url), "utf8");
  const deck = readFileSync(new URL("../src/live/SecondMonitorDeck.tsx", import.meta.url), "utf8");
  const liveCss = readFileSync(new URL("../src/live/live.css", import.meta.url), "utf8").replace(/\/\*[\s\S]*?\*\//g, "");
  const css = readFileSync(new URL("../src/live/secondMonitor.css", import.meta.url), "utf8").replace(/\/\*[\s\S]*?\*\//g, "");
  assert.equal(PINNED_SCOREBOARD_HEIGHT, 64);
  assert.equal(PINNED_DRAFT_MIN_MAP_H, 420);
  assert.match(deck, /const draftTableTooSmall = layout\?\.mode === "stacked" && draftPhase && !pinnedDraftFits\(layout\.map\.h\)/);
  assert.match(deck, /layout\.mode === "too_small" \|\| draftTableTooSmall/);
  assert.match(panel, /useCanvasBox\(minimap\.frameSize, layout === "pinned"\)/);
  assert.match(content, /fillStage\s*\?\s*\{\s*width:\s*"100%",\s*height:\s*"100%"[\s\S]*?objectFit:\s*"fill"/);
  assert.match(css, /\.sm-pinned \.live-map-canvas\s*\{[^}]*width:\s*100%[^}]*height:\s*100%/);
  assert.match(liveCss, /\.live-map-stage\s*\{[^}]*border:\s*1px\s+solid/);
  assert.match(css, /\.sm-host \*,[^}]*box-sizing:\s*border-box/);
  assert.match(css, /\.live-map-square-slot > \.live-map-stage,[\s\S]*?\.live-map-draft-slot > \.live-map-stage\s*\{[^}]*width:\s*100%;\s*height:\s*100%/);

  for (const [name, width, height] of windows) for (const screenSide of ["left", "right"] as const) {
    const placedLayout = placed(screenSide, width, height);
    const regionWidth = placedLayout.map.w - 12;
    const regionHeight = placedLayout.map.h - 12;
    for (const phase of ["live", "draft"] as const) {
      const edge = placedLayout.mapEdge;
      const contents = pinnedMapContents({ width: regionWidth, height: regionHeight, mapEdge: edge, phase });
      assert.ok(contents, `${name} ${screenSide} ${phase}: pinned content fits`);
      if (!contents) continue;
      const inside = (rect: Rect) => rect.x >= 0 && rect.y >= 0
        && rect.x + rect.w <= regionWidth && rect.y + rect.h <= regionHeight;
      assert.ok(inside(contents.scoreboard), `${name} ${screenSide} ${phase}: scoreboard stays inside the map region`);
      assert.equal(contents.scoreboard.h, PINNED_SCOREBOARD_HEIGHT);

      if (phase === "live") {
        assert.ok(contents.map, `${name} ${screenSide}: live map slot exists`);
        if (!contents.map) continue;
        assert.ok(inside(contents.map), `${name} ${screenSide}: map slot stays inside the map region`);
        assert.equal(contents.scoreboard.y + contents.scoreboard.h, contents.map.y, `${name} ${screenSide}: scoreboard touches the map slot`);
        assert.equal(contents.scoreboard.x, contents.map.x, `${name} ${screenSide}: scoreboard and map share an edge`);
        assert.equal(contents.scoreboard.w, contents.map.w, `${name} ${screenSide}: scoreboard and map have equal widths`);
        assert.equal(contents.map.w, contents.map.h, `${name} ${screenSide}: map slot stays square`);
        assert.equal(contents.map.w, Math.min(regionWidth, regionHeight - PINNED_SCOREBOARD_HEIGHT), `${name} ${screenSide}: map is the largest square after the scoreboard`);
        assert.equal(contents.map.x, edge === "right" ? regionWidth - contents.map.w : 0, `${name} ${screenSide}: map is pinned to the inner edge`);
        const canvas = { w: contents.map.w - 2, h: contents.map.h - 2 };
        assert.ok(Math.abs(canvas.w - contents.map.w) <= 2 && Math.abs(canvas.h - contents.map.h) <= 2,
          `${name} ${screenSide}: the 1 px stage border leaves the canvas within 2 px of its slot`);
      } else {
        assert.ok(pinnedDraftFits(placedLayout.map.h), `${name} ${screenSide}: requested draft board height remains legible`);
        assert.ok(contents.board, `${name} ${screenSide}: draft board exists`);
        if (!contents.board) continue;
        assert.ok(inside(contents.board), `${name} ${screenSide}: draft board stays inside the map region`);
        assert.equal(contents.scoreboard.y + contents.scoreboard.h, contents.board.y, `${name} ${screenSide}: scoreboard touches the draft board`);
        assert.equal(contents.board.x, 0);
        assert.equal(contents.board.w, regionWidth, `${name} ${screenSide}: draft board uses full map-region width`);
      }
    }
  }
  assert.equal(pinnedDraftFits(PINNED_DRAFT_MIN_MAP_H), true);
  assert.equal(pinnedDraftFits(PINNED_DRAFT_MIN_MAP_H - 1), false, "an undersized draft region uses the existing too-small state");
});

test("below the box needed for cards and map, use the existing too-small state", () => {
  assert.equal(secondMonitorLayout({ side: "right", width: 5 * CARD_MIN_W + 4 * CARD_GAP - 1, height: 900 }).mode, "too_small");
  assert.equal(secondMonitorLayout({ side: "right", width: 1200, height: 2 * CARD_ROW_MIN_H + MAP_PANEL_MIN_H + 2 * GAP - 1 }).mode, "too_small");
  assert.equal(secondMonitorLayout({ side: "off", width: 1200, height: 900 }).mode, "off");
});

test("layout is determined by the box and side only, never by card content", () => {
  const { width, height } = liveBox(1280, 720);
  assert.deepEqual(secondMonitorLayout({ side: "left", width, height }), secondMonitorLayout({ side: "left", width, height }));
  assert.deepEqual(secondMonitorLayout({ side: "left", width, height }).mode, "stacked");
});

const CENTER_WINDOWS: [string, number, number][] = [
  ["1920x1080", 1920, 1080], ["1920x1000", 1920, 1000], ["1600x900", 1600, 900],
  ["1366x768", 1366, 768], ["1280x720", 1280, 720],
];
const centerLiveBox = (w: number, h: number) => ({ width: w - 208, height: h - 132 });
const centerInside = (r: Rect, w: number, h: number) => r.x >= 0 && r.y >= 0 && r.x + r.w <= w && r.y + r.h <= h && r.w > 0 && r.h > 0;
const centerOverlap = (a: Rect, b: Rect) => Math.min(a.x + a.w, b.x + b.w) - Math.max(a.x, b.x) > 0 && Math.min(a.y + a.h, b.y + b.h) - Math.max(a.y, b.y) > 0;

test("centred mode places Radiant, minimap, and Dire without overflow at each supported viewport", () => {
  assert.equal(CENTER_ROOMY_CARD_MIN_H, 170);
  for (const [name, w, h] of CENTER_WINDOWS) {
    const { width, height } = centerLiveBox(w, h);
    const layout = centeredLiveLayout({ width, height });
    assert.equal(layout.mode, "field", `${name}: all three regions fit`);
    if (layout.mode !== "field") continue;
    for (const [region, rect] of [["Radiant", layout.radiant], ["map", layout.map], ["Dire", layout.dire]] as const) {
      assert.ok(centerInside(rect, width, height), `${name}: ${region} outside the Live box`);
    }
    assert.equal(layout.radiant.h, layout.map.h, `${name}: Radiant column matches map height`);
    assert.equal(layout.dire.h, layout.map.h, `${name}: Dire column matches map height`);
    assert.equal(layout.radiant.y, 0);
    assert.equal(layout.map.y, 0);
    assert.equal(layout.dire.y, 0);
    assert.ok(!centerOverlap(layout.radiant, layout.map));
    assert.ok(!centerOverlap(layout.map, layout.dire));
    assert.equal(layout.map.x, layout.radiant.w + CENTER_GAP);
    assert.equal(layout.dire.x, layout.map.x + layout.map.w + CENTER_GAP);
    assert.ok(layout.map.w >= CENTER_MAP_MIN_W);
    assert.ok(layout.card.w >= CENTER_CARD_MIN.w);
    assert.ok(layout.card.h >= (layout.card.w >= CENTER_WIDE_STATS_MIN_W ? CENTER_CARD_MIN.h : CENTER_CARD_NARROW_STATS_MIN_H));
    const expectedOwnMode = name === "1920x1080" || name === "1920x1000" ? "one-row-28"
      : name === "1280x720" ? "two-row-22" : "two-row-24";
    assert.equal(ownFieldCardMode(layout.card), expectedOwnMode, `${name}: own-card icons adapt to the measured dimensions`);
    const roomy = layout.card.w >= 320 && layout.card.h >= CENTER_ROOMY_CARD_MIN_H;
    assert.equal(roomy, name === "1920x1080" || name === "1920x1000", `${name}: expanded icon rows only activate above their measured floor`);
    assert.ok(layout.card.h * 5 + CENTER_CARD_GAP * 4 <= height, `${name}: five equal rows fit in each column`);
    const cards = [layout.radiant, layout.dire].flatMap(column => Array.from({ length: 5 }, (_, slot) => ({
      x: column.x, y: column.y + slot * (layout.card.h + CENTER_CARD_GAP), w: layout.card.w, h: layout.card.h,
    })));
    assert.equal(cards.length, 10, `${name}: both teams have five cards`);
    cards.forEach((card, index) => assert.ok(centerInside(card, width, height), `${name}: card ${index} outside the Live box`));
  }
});

test("centred field renders ten slot-ordered replay cards with same-height team columns and no duplicate status strip", () => {
  const deck = readFileSync(new URL("../src/deck/Deck.tsx", import.meta.url), "utf8");
  const cardCss = readFileSync(new URL("../src/live/heroCard.css", import.meta.url), "utf8").replace(/\/\*[\s\S]*?\*\//g, "");
  const liveCss = readFileSync(new URL("../src/live/secondMonitor.css", import.meta.url), "utf8").replace(/\/\*[\s\S]*?\*\//g, "");
  assert.match(deck, /orderBySlot\(\[ownModel, \.\.\.allyCards\]\)/);
  assert.match(deck, /<section className="deck-field-region deck-field-team"[^>]*data-region="radiant"/);
  assert.match(deck, /<section className="deck-field-region deck-field-team"[^>]*data-region="dire"/);
  assert.match(deck, /<section className="deck-field-region deck-field-map"[^>]*data-region="map"/);
  assert.doesNotMatch(deck, /gsi-phase-strip|deck-field-phase/);
  assert.match(deck, /draft \? <>[\s\S]*?<div className="deck-field-draft-row"/);
  assert.match(deck, /<DraftBarStrip frame=\{draftBar\}/);
  assert.match(deck, /<DraftDeckPanel visible \/>/);
  assert.match(deck, /<OwnHeroCard[^>]*fieldCard/);
  assert.match(deck, /ownTeam === "radiant" \? ourCards : enemyCards/);
  assert.match(deck, /ownTeam === "dire" \? ourCards : enemyCards/);
  assert.match(deck, /label=\{cardsText\.radiant\}/);
  assert.match(deck, /label=\{cardsText\.dire\}/);
  assert.match(deck, /layout\.card\.h >= CENTER_ROOMY_CARD_MIN_H/);
  assert.match(deck, /sideWidth >= 320/);
  const cardSource = readFileSync(new URL("../src/live/HeroCard.tsx", import.meta.url), "utf8");
  assert.match(cardSource, /lhc-list-field-\$\{fieldDirection\}/);
  assert.match(cardCss, /\.lhc-list-field-row\s*\{[^}]*grid-template-columns:\s*repeat\(5,/);
  assert.match(cardCss, /\.lhc-list-field\s*\{[^}]*grid-template-rows:\s*repeat\(5,\s*minmax\(0,\s*1fr\)\)/);
  assert.match(liveCss, /\.deck-field-map\s*\{[^}]*display:\s*flex/);
  assert.match(liveCss, /\.deck-field-region\s*\{[^}]*overflow:\s*hidden/);
  assert.match(liveCss, /\.app\[data-page="live"\] \.shell-live-page > \.deck-field-root/);
  assert.match(liveCss, /\.app\[data-page="live"\] \.deck-field-draft-row \.draft-column-surface-deck\s*\{[^}]*overflow:\s*hidden/);
  assert.doesNotMatch(liveCss, /\.deck-field-phase-reason\s*\{/);
  assert.doesNotMatch(liveCss, /overflow(?:-[xy])?:\s*(?:auto|scroll)/);
});

test("own compact stat grid uses 2x4 at both target viewports and falls back below its measured threshold", () => {
  const targetCards: { name: string; card: { w: number; h: number } }[] = [];
  for (const [name, width, height] of [["pinned 1920x1080", 1920, 1080], ["pinned 1440x900", 1440, 900]] as const) {
    targetCards.push({ name, card: placed("right", width, height).card });
  }
  for (const [name, width, height] of [["centred 1920x1080", 1920, 1080], ["centred 1440x900", 1440, 900]] as const) {
    const box = centerLiveBox(width, height);
    const layout = centeredLiveLayout(box);
    assert.equal(layout.mode, "field", `${name}: card geometry is available`);
    if (layout.mode === "field") targetCards.push({ name, card: layout.card });
  }

  assert.equal(FIELD_TWO_ROW_24_MIN_W, 254);
  const css = readFileSync(new URL("../src/live/heroCard.css", import.meta.url), "utf8").replace(/\/\*[\s\S]*?\*\//g, "");
  assert.match(css, /\.lhc-field-card\.lhc-field-own \.lhc-stats\.is-own-compact\s*\{[^}]*grid-template-columns:\s*repeat\(4,\s*minmax\(0,\s*1fr\)\)[^}]*grid-template-rows:\s*repeat\(2,\s*minmax\(12px,\s*1fr\)\)/,
    "the compact own grid has four 12 px columns and two 12 px rows");
  assert.match(css, /\.lhc-field-card\.lhc-field-own\.is-own-two-row-22 \.lhc-stats\s*\{[^}]*grid-template-columns:\s*repeat\(5,[^}]*grid-template-rows:\s*repeat\(3,/,
    "narrow cards preserve the existing complete five-by-three grid");
  assert.match(css, /\.lhc-field-card\.lhc-field-own \.lhc-stat dt, \.lhc-field-card\.lhc-field-own \.lhc-stat dd\s*\{[^}]*font-size:\s*12px/,
    "stat text remains at the 12 px minimum");
  assert.match(css, /\.lhc-field-card\.lhc-field-own\.is-own-compact\s*\{[^}]*height:\s*min-content;\s*align-self:\s*start/,
    "the own card shrinks within its existing team slot instead of filling the complete row height");

  for (const { name, card } of targetCards) {
    const mode = ownFieldCardMode(card);
    const grid = heroCardModel.ownFieldStatsLayout?.(mode);
    assert.equal(grid?.columns, 4, `${name}: four stat columns`);
    assert.equal(grid?.rows, 2, `${name}: two stat rows`);
    assert.equal(grid?.statHeight, 24, `${name}: two 12 px rows`);
    const innerWidth = card.w - 13;
    const gap = 4;
    const cellWidth = (innerWidth - gap * 3) / 4;
    assert.ok(cellWidth >= 50, `${name}: ${cellWidth}px per cell fits a 12 px icon and five-character value`);
    const rects = Array.from({ length: 8 }, (_, index) => ({
      x: (index % 4) * (cellWidth + gap),
      y: Math.floor(index / 4) * 12,
      w: cellWidth,
      h: 12,
    }));
    for (let left = 0; left < rects.length; left++) for (let right = left + 1; right < rects.length; right++) {
      const a = rects[left]!, b = rects[right]!;
      assert.ok(a.x + a.w <= b.x || b.x + b.w <= a.x || a.y + a.h <= b.y || b.y + b.h <= a.y,
        `${name}: compact stat cells ${left} and ${right} do not overlap`);
    }
    const newMinHeight = minOwnFieldCardHeight(mode, grid!.statHeight);
    assert.ok(newMinHeight < card.h, `${name}: ${newMinHeight}px own card is shorter than its ${card.h}px layout row`);
    assert.equal(newMinHeight, mode === "one-row-28" ? 102 : 112, `${name}: the compact own card's minimum height is predictable`);
  }

  const belowThreshold = ownFieldCardMode({ w: FIELD_TWO_ROW_24_MIN_W - 1, h: 160 });
  assert.equal(belowThreshold, "two-row-22");
  const fallback = heroCardModel.ownFieldStatsLayout?.(belowThreshold);
  assert.deepEqual(fallback, { compact: false, columns: 5, rows: 3, statHeight: 36 });
  assert.equal(ownFieldCardMode({ w: FIELD_TWO_ROW_24_MIN_W, h: 124 }), "two-row-24");
  assert.equal(ownFieldCardMode({ w: FIELD_TWO_ROW_24_MIN_W, h: 123 }), "two-row-22", "the 2x4 24 px mode needs 124 px of card height");
  assert.equal(heroCardModel.ownFieldStatsLayout?.(ownFieldCardMode({ w: 467, h: 102 })).compact, true,
    "the wider 28 px mode can fit its compact grid without the 124 px height threshold");
});

function minOwnFieldCardHeight(mode: ReturnType<typeof ownFieldCardMode>, statHeight: number): number {
  if (mode === "one-row-28") return 24 + 14 + 28 + statHeight + 3 * 2 + 2 * 2 + 2;
  if (mode === "two-row-24") return 20 + 14 + 24 + 24 + statHeight + 4 + 2;
  return 18 + 14 + 22 + 22 + statHeight + 2;
}

test("undersized centred boxes use a clear state rather than scrolling or clipping cards", () => {
  assert.deepEqual(centeredLiveLayout({ width: 700, height: 500 }), { mode: "too_small" });
  assert.deepEqual(centeredLiveLayout({ width: 950, height: 700 }), { mode: "too_small" }, "narrow cards need enough room for every 12 px stat row");
  assert.equal(centeredLiveLayout({ width: 950, height: 796 }).mode, "field", "narrow stat rows fit at their measured floor");
  assert.deepEqual(centeredLiveLayout({ width: 950, height: 795 }), { mode: "too_small" }, "never compress five stat rows below the narrow-card floor");
});
