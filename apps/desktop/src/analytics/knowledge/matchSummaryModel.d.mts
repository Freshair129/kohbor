import type { MapCalibration } from "../../replay/replayPack.mjs";

export type MatchSummarySide = "radiant" | "dire";
export interface MatchSummaryMetric { value: number | null; reason: string | null }
export interface MatchSummaryMetrics {
  k: MatchSummaryMetric;
  d: MatchSummaryMetric;
  a: MatchSummaryMetric;
  nw: MatchSummaryMetric;
  lh: MatchSummaryMetric;
  dn: MatchSummaryMetric;
  gpm: MatchSummaryMetric;
  xpm: MatchSummaryMetric;
  heroDamage: MatchSummaryMetric;
}
export interface MatchSummaryHeroCard {
  heroId: string | null;
  heroIconName: string | null;
  hero: string | null;
  side: MatchSummarySide | null;
  slot: number | null;
  lane: string | null;
  laneReason: string | null;
  laneResult: string | null;
  laneResultReason: string | null;
  kda: { k: MatchSummaryMetric; d: MatchSummaryMetric; a: MatchSummaryMetric };
  economyLabel: "มูลค่าสุทธิ" | "ทองที่หาได้";
  economy: MatchSummaryMetric;
}
export interface MatchSummaryItem {
  id: string;
  iconName: string | null;
  purchaseTimeSeconds: number | null;
  purchaseMinute: number | null;
}
export interface MatchSummaryScoreboardRow {
  heroId: string | null;
  heroIconName: string | null;
  hero: string | null;
  slot: number | null;
  metrics: MatchSummaryMetrics;
  items: MatchSummaryItem[];
  itemsKnown: boolean;
  itemsReason: string | null;
}
export interface MatchSummaryTeamScoreboard {
  rows: MatchSummaryScoreboardRow[];
  totals: { metrics: MatchSummaryMetrics; playerCount: number };
}
export interface MatchSummaryDraftEntry {
  order: number;
  side: MatchSummarySide | "unknown";
  kind: "Pick" | "Ban" | "— ไม่ทราบ";
  heroId: string | null;
  heroIconName: string | null;
  hero: string;
}
export interface MatchSummaryViewModel {
  header: {
    winner: MatchSummarySide | null;
    winnerText: string;
    winnerSource: string | null;
    warning: string | null;
    kills: { radiant: MatchSummaryMetric; dire: MatchSummaryMetric };
    durationSeconds: number | null;
    gameMode: number | string | null;
    gameModeReason: string | null;
    patch: string | null;
    patchReason: string | null;
  };
  cards: Record<MatchSummarySide, MatchSummaryHeroCard[]>;
  scoreboard: Record<MatchSummarySide, MatchSummaryTeamScoreboard>;
  draft: {
    state: "unknown" | "empty" | "available";
    reason: string | null;
    groups: Record<MatchSummarySide | "unknown", MatchSummaryDraftEntry[]>;
  };
}

export function buildMatchSummaryViewModel(digest: unknown): MatchSummaryViewModel;
export function buildingMapPoint(
  position: unknown,
  calibration: MapCalibration | null | undefined,
): { x: number; y: number } | null;
