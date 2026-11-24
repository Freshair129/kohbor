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
  GAP,
  MAP_PANEL_MIN_H,
  centeredLiveLayout,
  ownFieldCardMode,
  placedRegions,
  secondMonitorLayout,
  type PlacedLayout,
  type Rect,
} from "../src/live/secondMonitorLayout.ts";

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
  assert.match(css, /\.lhc-field-card\.lhc-field-own\s*\{[^}]*minmax\(86px,\s*1fr\)/, "the narrow own card reserves its basis line and all three 24 px stat rows");
  assert.match(css, /\.lhc-field-card\.lhc-field-own\.has-wide-stats\s*\{[^}]*minmax\(48px,\s*1fr\)/, "wide own cards retain two complete 24 px stat rows");
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
  assert.match(panel, /className="live-map-body is-pinned" data-map-edge=\{mapEdge\}/);
  assert.match(panel, /<div className="live-map-square-slot">\{stage\}<\/div>/);
  assert.doesNotMatch(deck, /data-region="slot"|sm-slot|EventFeed|AnnouncerStatus|GsiSetupHint/);
  assert.doesNotMatch(css, /overflow(?:-[xy])?:\s*(?:auto|scroll)/, "pinned Live has no scroll containers");
  assert.match(css, /\.sm-host\s*\{[^}]*overflow:\s*hidden/);
  assert.match(css, /\.sm-card-row > \.lhc-list-field\s*\{[^}]*height:\s*100%/);
  assert.match(css, /\.sm-map-region > \.live-pinned-panel\s*\{[^}]*height:\s*100%/);
  assert.match(css, /\.live-map-body\.is-pinned\[data-map-edge="right"\] \.live-map-toolbar\s*\{[^}]*grid-column:\s*1/);
  assert.match(css, /\.live-map-body\.is-pinned\[data-map-edge="left"\] \.live-map-toolbar\s*\{[^}]*grid-column:\s*2/);
  assert.match(css, /width:\s*min\(100cqw,\s*100cqh\)/, "the minimap uses the largest square in its region");
  assert.match(cardCss, /\.lhc-field-card\.is-compact-fit \.lhc-side\s*\{[^}]*gap:\s*0/,
    "the 22 px item row fits the minimum pinned card track without horizontal clipping");
  assert.match(css, /.sm-host \.lhc-field-card\.is-roomy-field\s*\{[^}]*28px 12px 16px 28px 28px minmax\(36px,\s*1fr\)/,
    "pinned roomy rows preserve three 12 px stat lines inside their 160 px row cap");
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

test("centred field renders ten slot-ordered replay cards, same-height team columns, and the GSI phase strip", () => {
  const deck = readFileSync(new URL("../src/deck/Deck.tsx", import.meta.url), "utf8");
  const cardCss = readFileSync(new URL("../src/live/heroCard.css", import.meta.url), "utf8").replace(/\/\*[\s\S]*?\*\//g, "");
  const liveCss = readFileSync(new URL("../src/live/secondMonitor.css", import.meta.url), "utf8").replace(/\/\*[\s\S]*?\*\//g, "");
  assert.match(deck, /orderBySlot\(\[ownModel, \.\.\.allyCards\]\)/);
  assert.match(deck, /<section className="deck-field-region deck-field-team"[^>]*data-region="radiant"/);
  assert.match(deck, /<section className="deck-field-region deck-field-team"[^>]*data-region="dire"/);
  assert.match(deck, /<section className="deck-field-region deck-field-map"[^>]*data-region="map"/);
  assert.match(deck, /data-testid="gsi-phase-strip"/);
  assert.match(deck, /draft \? <div className="deck-field-draft-row"/);
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
  assert.match(liveCss, /\.deck-field-phase-reason\s*\{[^}]*text-overflow:\s*ellipsis/);
  assert.doesNotMatch(liveCss, /overflow(?:-[xy])?:\s*(?:auto|scroll)/);
});

test("undersized centred boxes use a clear state rather than scrolling or clipping cards", () => {
  assert.deepEqual(centeredLiveLayout({ width: 700, height: 500 }), { mode: "too_small" });
  assert.deepEqual(centeredLiveLayout({ width: 950, height: 700 }), { mode: "too_small" }, "narrow cards need enough room for every 12 px stat row");
  assert.equal(centeredLiveLayout({ width: 950, height: 796 }).mode, "field", "narrow stat rows fit at their measured floor");
  assert.deepEqual(centeredLiveLayout({ width: 950, height: 795 }), { mode: "too_small" }, "never compress five stat rows below the narrow-card floor");
});
