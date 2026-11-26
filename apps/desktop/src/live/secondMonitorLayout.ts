// Second-monitor Live geometry (UI-016; owner redesign, 2026-11-24).
// The box alone determines the map and the two fixed five-card rows. No content scrolls and card content cannot move
// or resize the map. Coordinates are measured from the inner edge (the edge touching the main monitor).

export interface Rect { x: number; y: number; w: number; h: number }
export type CardKind = "row";
export type PlacedLayout = {
  mode: "stacked";
  mapEdge: "left" | "right";
  /** Full-width map panel between the enemy row and our row; the captured square is pinned to mapEdge. */
  map: Rect;
  ours: Rect;
  enemy: Rect;
  cardKind: CardKind;
  card: { w: number; h: number };
};
export type SecondMonitorLayout = { mode: "off" } | { mode: "too_small" } | PlacedLayout;

export const GAP = 8;
/** Gap between the five card tracks in each pinned row, matching `.lhc-list-field`. */
export const CARD_GAP = 4;
/** Minimum row height includes 12 px basis + 3 × 24 px stat rows, card tracks, padding and borders. */
export const CARD_ROW_MIN_H = 156;
/** The expanded card rows need 28 px icons, three 12 px stat rows and 1 px section gaps. */
export const CARD_ROOMY_ROW_MIN_H = 157;
/** Minimum track width for six 22 px items, backpack, TP and neutral slots without clipping. */
export const CARD_MIN_W = 224;
/** Map panel minimum: a 220 px captured map stage plus its fixed toolbar and padding. */
export const MAP_PANEL_MIN_H = 260;
export const CARD_ROW_MAX_H = 160;
/** The pinned scoreboard has one score row and one momentum row inside its 64 px toolbar. */
export const PINNED_SCOREBOARD_HEIGHT = 64;
/** Minimum map-region height that leaves room for both uncut draft rows below the scoreboard. */
export const PINNED_DRAFT_MIN_MAP_H = 420;

function innerEdge(side: Exclude<SecondMonitorSide, "off">): "left" | "right" {
  return side === "right" ? "left" : "right";
}

export type SecondMonitorSide = "off" | "left" | "right";

/** Places enemy cards above and our cards below a full-width panel whose minimap stays on the inner edge. */
export function secondMonitorLayout(input: { side: SecondMonitorSide; width: number; height: number }): SecondMonitorLayout {
  const { side } = input;
  if (side === "off") return { mode: "off" };
  const w = Math.floor(input.width), h = Math.floor(input.height);
  if (!Number.isFinite(w) || !Number.isFinite(h) || w < 5 * CARD_MIN_W + 4 * CARD_GAP || h < 2 * CARD_ROW_MIN_H + MAP_PANEL_MIN_H + 2 * GAP) {
    return { mode: "too_small" };
  }
  const availableForRows = Math.floor((h - MAP_PANEL_MIN_H - 2 * GAP) / 2);
  const rowH = Math.min(CARD_ROW_MAX_H, availableForRows);
  if (rowH < CARD_ROW_MIN_H) return { mode: "too_small" };
  const mapH = h - 2 * rowH - 2 * GAP;
  if (mapH < MAP_PANEL_MIN_H) return { mode: "too_small" };
  return {
    mode: "stacked",
    mapEdge: innerEdge(side),
    enemy: { x: 0, y: 0, w, h: rowH },
    map: { x: 0, y: rowH + GAP, w, h: mapH },
    ours: { x: 0, y: rowH + GAP + mapH + GAP, w, h: rowH },
    cardKind: "row",
    card: { w: Math.floor((w - 4 * CARD_GAP) / 5), h: rowH },
  };
}

/** Regions present in a placed layout, for the view and for pure-geometry tests. */
export function placedRegions(layout: PlacedLayout): [string, Rect][] {
  return [["enemy", layout.enemy], ["map", layout.map], ["ours", layout.ours]];
}

export interface PinnedMapContents {
  scoreboard: Rect;
  /** Square canvas slot in live phase. */
  map: Rect | null;
  /** Full-width table in draft phase. */
  board: Rect | null;
}

/** Places the pinned scoreboard directly above either the inner-edge map or the full-width draft board. */
export function pinnedMapContents(input: {
  width: number;
  height: number;
  mapEdge: "left" | "right";
  phase: "live" | "draft";
}): PinnedMapContents | null {
  const width = Math.floor(input.width);
  const height = Math.floor(input.height);
  if (!Number.isFinite(width) || !Number.isFinite(height) || width <= 0 || height <= PINNED_SCOREBOARD_HEIGHT) return null;

  if (input.phase === "draft") {
    return {
      scoreboard: { x: 0, y: 0, w: width, h: PINNED_SCOREBOARD_HEIGHT },
      map: null,
      board: { x: 0, y: PINNED_SCOREBOARD_HEIGHT, w: width, h: height - PINNED_SCOREBOARD_HEIGHT },
    };
  }

  const mapSize = Math.min(width, height - PINNED_SCOREBOARD_HEIGHT);
  const x = input.mapEdge === "right" ? width - mapSize : 0;
  return {
    scoreboard: { x, y: 0, w: mapSize, h: PINNED_SCOREBOARD_HEIGHT },
    map: { x, y: PINNED_SCOREBOARD_HEIGHT, w: mapSize, h: mapSize },
    board: null,
  };
}

export function pinnedDraftFits(mapRegionHeight: number): boolean {
  return Number.isFinite(mapRegionHeight) && mapRegionHeight >= PINNED_DRAFT_MIN_MAP_H;
}

// Centred replay-style Live field geometry (UI-002; owner redesign, 2026-11-24).
// Each side has five slot-ordered cards and the same total height as the central map panel.
export interface CenterRect { x: number; y: number; w: number; h: number }
export interface CenteredLiveLayout {
  mode: "field";
  map: CenterRect;
  radiant: CenterRect;
  dire: CenterRect;
  card: { w: number; h: number };
}
export type CenteredLiveLayoutResult = CenteredLiveLayout | { mode: "too_small" };
export type OwnFieldCardMode = "one-row-28" | "two-row-24" | "two-row-22";

export const CENTER_GAP = 8;
export const CENTER_CARD_GAP = 4;
export const CENTER_WIDE_STATS_MIN_W = 260;
export const CENTER_CARD_NARROW_STATS_MIN_H = 156;
/** The expanded centred card needs 28 px icons, three dense 12 px stat rows and all replay sections. */
export const CENTER_ROOMY_CARD_MIN_H = 170;
/** One combined 28 px ability/item row only fits a centered team column at this measured width. */
export const CENTER_ONE_ROW_28_MIN_W = 467;
/** Two 24 px item rows need this card width after card borders and padding. */
export const FIELD_TWO_ROW_24_MIN_W = 254;
/** Two 24 px ability and item rows need a 124 px card when headers, stats, borders and tight gaps are included. */
export const CENTER_TWO_ROW_24_MIN_H = 124;
/** Minimum compact field-card dimensions preserve all replay sections and 12 px text. */
export const CENTER_CARD_MIN = { w: 224, h: 114 } as const;
export const CENTER_MAP_MIN_W = 280;

/** Scale the own card's icon rows to the measured width and height without dropping a section. */
export function ownFieldCardMode(card: { w: number; h: number }): OwnFieldCardMode {
  if (card.w >= CENTER_ONE_ROW_28_MIN_W) return "one-row-28";
  if (card.w >= FIELD_TWO_ROW_24_MIN_W && card.h >= CENTER_TWO_ROW_24_MIN_H) return "two-row-24";
  return "two-row-22";
}

/** The map panel and both team columns are placed only from the measured Live box. */
export function centeredLiveLayout(input: { width: number; height: number }): CenteredLiveLayoutResult {
  const w = Math.floor(input.width), h = Math.floor(input.height);
  const mapMax = w - 2 * CENTER_CARD_MIN.w - 2 * CENTER_GAP;
  const mapW = Math.min(h, Math.floor(w * 0.44), mapMax);
  const sideW = Math.floor((w - mapW - 2 * CENTER_GAP) / 2);
  const cardsHeight = Math.floor((h - 4 * CENTER_CARD_GAP) / 5);
  const minimumCardHeight = sideW >= CENTER_WIDE_STATS_MIN_W ? CENTER_CARD_MIN.h : CENTER_CARD_NARROW_STATS_MIN_H;
  if (!Number.isFinite(w) || !Number.isFinite(h) || sideW < CENTER_CARD_MIN.w || mapW < CENTER_MAP_MIN_W || cardsHeight < minimumCardHeight) {
    return { mode: "too_small" };
  }
  return {
    mode: "field",
    radiant: { x: 0, y: 0, w: sideW, h },
    map: { x: sideW + CENTER_GAP, y: 0, w: mapW, h },
    dire: { x: sideW + CENTER_GAP + mapW + CENTER_GAP, y: 0, w: w - sideW - CENTER_GAP - mapW - CENTER_GAP, h },
    card: { w: sideW, h: cardsHeight },
  };
}
