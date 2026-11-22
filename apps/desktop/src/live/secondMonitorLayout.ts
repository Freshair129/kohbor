// Responsive second-monitor layout (UI-016; owner feedback on build 87d89d6, 2026-11-23).
// Pure geometry: every region is placed from the Live box size alone, so nothing on the page scrolls and the map never
// moves because of content (UI-002). Coordinates are measured from the INNER edge (the edge touching the main monitor);
// the view mirrors them when the map is pinned right.
// @ts-ignore Node's built-in TypeScript loader requires the explicit source extension.
import { mapEdgeFor, type SecondMonitorSide } from "./secondMonitorModel.ts";

export interface Rect { x: number; y: number; w: number; h: number }
export type CardKind = "row" | "tile";
export type WidePlan = "both+slot" | "both" | "ours";

export type PlacedLayout = {
  mode: "wide" | "stacked";
  mapEdge: "left" | "right";
  /** The map pane square (toolbar included) and the quick strip under it. */
  map: Rect;
  band: Rect;
  /** Status chips, start button and the side selector. */
  status: Rect;
  ours: Rect;
  /** Null only when the box is too narrow for both teams: our team is then shown complete on its own. */
  enemy: Rect | null;
  /** The free slot (event log today; voice or agent chat later). Null when there is no room. */
  slot: Rect | null;
  cardKind: CardKind;
  card: { w: number; h: number };
  plan: WidePlan | "stacked";
};
export type SecondMonitorLayout = { mode: "off" } | { mode: "too_small" } | PlacedLayout;

export const GAP = 8;
/** Team heading (title and source line) and the gap under it; gap between cards. */
export const HEAD = 22;
export const HEAD_GAP = 6;
export const CARD_GAP = 6;
/**
 * Status chips, the start button and the side selector wrap to more lines in a narrow box. Heights measured in the app
 * (padding included, no clipping): one chip line from 560 px, two from 260 px, and the selector wraps below that.
 */
export function statusHeightFor(width: number): number {
  return width >= 560 ? 88 : width >= 260 ? 124 : 152;
}
export const BAND_MIN = 84;
export const BAND_MAX = 156;
export const MAP_MIN = 360;
export const SLOT_MIN_W = 240;
export const SLOT_MIN_H = 120;
/** Card floors measured in the mock (no clipping at 12 px text): row card in columns, tile card in rows. */
export const ROW_CARD_MIN = { w: 232, h: 88 } as const;
export const TILE_CARD_MIN = { w: 132, h: 150 } as const;
export const TILE_CARD_COMFY = 170;
export const TILE_CARD_MAX = 220;
/** Below this aspect ratio (width / height) the stacked layout is tried first. */
export const STACK_ASPECT = 1.25;

export function bandFor(map: number): number {
  return Math.round(Math.max(BAND_MIN, Math.min(BAND_MAX, map * 0.15)));
}

/** Largest map square that leaves room for its quick strip under it within `height`. */
function mapFittingHeight(height: number): number {
  let m = Math.floor(height - BAND_MIN);
  while (m > 0 && m + bandFor(m) > height) m -= 1;
  return m;
}

const groupHeight = (cardH: number) => HEAD + HEAD_GAP + 5 * cardH + 4 * CARD_GAP;
const cardHeightFor = (groupH: number) => Math.floor((groupH - HEAD - HEAD_GAP - 4 * CARD_GAP) / 5);

function wide(w: number, h: number, mapEdge: "left" | "right"): PlacedLayout | null {
  const ideal = mapFittingHeight(h);
  const plans: { plan: WidePlan; cols: 1 | 2; slot: boolean }[] = [
    { plan: "both+slot", cols: 2, slot: true }, { plan: "both", cols: 2, slot: false }, { plan: "ours", cols: 1, slot: false },
  ];
  for (const { plan, cols, slot } of plans) {
    const needOuter = cols * ROW_CARD_MIN.w + (cols - 1) * GAP + (slot ? SLOT_MIN_W + GAP : 0);
    const m = Math.min(ideal, w - needOuter - GAP);
    // The free slot is optional: it may never shrink the map.
    if (m < MAP_MIN || (slot && m < ideal)) continue;
    const outerX = m + GAP;
    const outerW = w - outerX;
    const statusH = statusHeightFor(outerW);
    const groupH = h - statusH - GAP;
    const cardH = cardHeightFor(groupH);
    if (cardH < ROW_CARD_MIN.h) continue;
    const colW = slot ? Math.min(420, Math.floor((outerW - SLOT_MIN_W - 2 * GAP) / 2)) : Math.floor((outerW - (cols - 1) * GAP) / cols);
    const top = statusH + GAP;
    const ours = { x: outerX, y: top, w: colW, h: groupHeight(cardH) };
    const enemy = cols === 2 ? { x: outerX + colW + GAP, y: top, w: colW, h: groupHeight(cardH) } : null;
    const slotX = outerX + 2 * (colW + GAP);
    return {
      mode: "wide", mapEdge, plan,
      map: { x: 0, y: 0, w: m, h: m }, band: { x: 0, y: m, w: m, h: bandFor(m) },
      status: { x: outerX, y: 0, w: outerW, h: statusH },
      ours, enemy, slot: slot ? { x: slotX, y: top, w: w - slotX, h: groupH } : null,
      cardKind: "row", card: { w: colW, h: cardH },
    };
  }
  return null;
}

function stacked(w: number, h: number, mapEdge: "left" | "right"): PlacedLayout | null {
  const cardW = Math.floor((w - 4 * CARD_GAP) / 5);
  if (cardW < TILE_CARD_MIN.w) return null;
  for (const withSlot of [true, false]) {
    const mW = withSlot ? w - SLOT_MIN_W - GAP : w;
    const statusH = statusHeightFor(withSlot ? w - mW - GAP : w);
    const statusRow = withSlot ? 0 : statusH + GAP;
    const roomy = Math.floor((h - statusRow - mW - bandFor(mW) - 2 * (HEAD + HEAD_GAP) - 2 * GAP) / 2);
    const start = Math.min(TILE_CARD_MAX, Math.max(TILE_CARD_COMFY, roomy));
    let best: { rowH: number; m: number } | null = null;
    // Rows start comfortable and shrink toward the floor only while that grows the map noticeably.
    for (let rowH = start; rowH >= TILE_CARD_MIN.h; rowH -= 2) {
      const midH = h - statusRow - 2 * (rowH + HEAD + HEAD_GAP) - 2 * GAP;
      const m = Math.min(mW, mapFittingHeight(midH));
      if (m >= MAP_MIN && (!best || m > best.m + 40)) best = { rowH, m };
      if (best && best.m >= mW) break;
    }
    if (!best) continue;
    const { rowH, m } = best;
    const rowBlock = rowH + HEAD + HEAD_GAP;
    const enemyY = statusRow;
    const midTop = enemyY + rowBlock + GAP;
    const midH = h - midTop - rowBlock - GAP;
    const band = bandFor(m);
    const mapY = midTop + Math.max(0, Math.floor((midH - m - band) / 2));
    const slotX = m + GAP;
    const slotW = w - slotX;
    const slotStatusH = statusHeightFor(slotW);
    const slotRect = withSlot ? { x: slotX, y: midTop + slotStatusH + GAP, w: slotW, h: midH - slotStatusH - GAP } : null;
    return {
      mode: "stacked", mapEdge, plan: "stacked",
      map: { x: 0, y: mapY, w: m, h: m }, band: { x: 0, y: mapY + m, w: m, h: band },
      status: withSlot ? { x: slotX, y: midTop, w: slotW, h: slotStatusH } : { x: 0, y: 0, w, h: statusH },
      enemy: { x: 0, y: enemyY, w, h: rowBlock },
      ours: { x: 0, y: h - rowBlock, w, h: rowBlock },
      slot: slotRect && slotRect.h >= SLOT_MIN_H ? slotRect : null,
      cardKind: "tile", card: { w: cardW, h: rowH },
    };
  }
  return null;
}

/**
 * Chooses the layout for the Live box. Wide (option B) for landscape boxes, stacked (enemy row, map with the free slot,
 * our row) for portrait or half-width boxes; when neither fits its floors the page shows "window too small".
 * Priority when space runs out: our five cards, the map (down to its floor), the enemy cards, then the free slot.
 */
export function secondMonitorLayout(input: { side: SecondMonitorSide; width: number; height: number }): SecondMonitorLayout {
  const { side } = input;
  if (side === "off") return { mode: "off" };
  const w = Math.floor(input.width), h = Math.floor(input.height);
  if (!Number.isFinite(w) || !Number.isFinite(h) || w <= 0 || h <= 0) return { mode: "too_small" };
  const edge = mapEdgeFor(side);
  const order = w / h >= STACK_ASPECT ? [wide, stacked] : [stacked, wide];
  for (const place of order) {
    const layout = place(w, h, edge);
    if (layout) return layout;
  }
  return { mode: "too_small" };
}

/** Regions present in a placed layout, for the view and for tests. */
export function placedRegions(layout: PlacedLayout): [string, Rect][] {
  const regions: [string, Rect | null][] = [["map", layout.map], ["band", layout.band], ["status", layout.status],
    ["ours", layout.ours], ["enemy", layout.enemy], ["slot", layout.slot]];
  return regions.filter((entry): entry is [string, Rect] => entry[1] !== null);
}

/** Card sizing for the compact card (CSS variables): portrait, name, bar and skill-square sizes grow with the card. */
export function cardMetrics(kind: CardKind, card: { w: number; h: number }) {
  if (kind === "row") {
    const portrait = Math.max(40, Math.min(64, card.h - 24));
    return { portrait, name: card.h >= 100 ? 15 : 14, bar: card.h >= 104 ? 14 : 12, square: card.h >= 104 ? 30 : 22 };
  }
  return {
    portrait: Math.max(40, Math.min(96, card.h - 116)), name: 15, bar: 14,
    square: Math.max(22, Math.min(32, Math.floor((card.w - 30) / 4))),
  };
}
