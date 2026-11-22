// Broadcast-style draft board in the pinned map area (UI-019; owner request 2026-11-23).
// Two rows (one per team), five slots each. Identity comes only from Draft Bar 5b in the same session; player names
// only from Draft 5c (local OCR, memory-only, deck-only, behind the visible "show player names" setting); win rates
// only from the hero statistics projection (DEC-PICK-STATS, separate consent). Anything missing is null ("—").
// @ts-ignore Node's built-in TypeScript loader requires the explicit source extension.
import { displayHeroName } from "../deck/heroName.ts";
// @ts-ignore Node's built-in TypeScript loader requires the explicit source extension.
import type { DraftRosterMemory, Team } from "./secondMonitorModel.ts";

/** Estimated position group 1..5 from the statistics position groups; always an estimate. */
export type PositionGroup = 1 | 2 | 3 | 4 | 5;

/**
 * Per-slot statistics a host projection may supply (DEC-PICK-STATS). Not produced on this branch.
 * `win_rate_tenths` is a hero's historical win rate (e.g. 523 = 52.3 %), never a win chance for this match.
 */
export interface DraftSlotStats { win_rate_tenths: number | null; position: PositionGroup | null; low_data: boolean }
export type DraftStatsInput = Partial<Record<number, DraftSlotStats | null>>;

/**
 * Player names from Draft 5c, keyed by slot. Not implemented in the product yet; when it is, names stay in memory for
 * the current match only and are passed here only while the visible setting is on.
 */
export type PlayerNamesInput = Partial<Record<number, string | null>>;

export interface DraftBoardSlot {
  slot: number;
  team: Team;
  ours: boolean | null;
  heroId: string | null;
  heroName: string | null;
  playerName: string | null;
  /** 0..100 with one decimal, or null. */
  winRate: number | null;
  position: PositionGroup | null;
  lowData: boolean;
}
export interface DraftBoardRow { team: Team; ours: boolean | null; slots: DraftBoardSlot[] }

const NAME_MAX = 32;

function cleanName(value: string | null | undefined): string | null {
  if (typeof value !== "string") return null;
  const trimmed = value.replace(/[\u0000-\u001f\u007f]/g, "").trim();
  return trimmed.length > 0 ? trimmed.slice(0, NAME_MAX) : null;
}

function cleanStats(stats: DraftSlotStats | null | undefined) {
  const tenths = stats?.win_rate_tenths;
  const winRate = typeof tenths === "number" && Number.isInteger(tenths) && tenths >= 0 && tenths <= 1000 ? tenths / 10 : null;
  const position = stats && [1, 2, 3, 4, 5].includes(stats.position as number) ? stats.position : null;
  return { winRate, position, lowData: stats?.low_data === true };
}

/** Radiant row first, Dire row second, like a broadcast; `ours` marks the own side when it is known. */
export function draftBoardRows(input: {
  memory: DraftRosterMemory; epoch: number | null; ownTeam: Team | null;
  stats?: DraftStatsInput | null; names?: PlayerNamesInput | null; showNames: boolean;
}): DraftBoardRow[] {
  const current = input.memory.epoch !== null && input.memory.epoch === input.epoch;
  return (["radiant", "dire"] as const).map(team => {
    const ours = input.ownTeam === null ? null : input.ownTeam === team;
    const base = team === "radiant" ? 0 : 5;
    const slots = [0, 1, 2, 3, 4].map((offset): DraftBoardSlot => {
      const slot = base + offset;
      const heroId = current ? input.memory.slots[slot] ?? null : null;
      const heroName = displayHeroName(heroId)?.name ?? null;
      // Stats belong to a picked hero; without an identity they are not shown.
      const stats = heroName ? cleanStats(input.stats?.[slot]) : { winRate: null, position: null, lowData: false };
      return {
        slot, team, ours, heroId: heroName ? heroId : null, heroName,
        playerName: input.showNames ? cleanName(input.names?.[slot]) : null,
        ...stats,
      };
    });
    return { team, ours, slots };
  });
}

export function formatWinRate(value: number | null): string {
  return value === null ? "—" : `${value.toFixed(1)}%`;
}

export function draftBoardCopy(locale: "th" | "en") {
  const th = locale === "th";
  return {
    title: th ? "ดราฟต์" : "Draft",
    radiant: "Radiant",
    dire: "Dire",
    ours: th ? "ทีมเรา" : "Our team",
    theirs: th ? "ฝั่งตรงข้าม" : "Enemy team",
    player: th ? "ผู้เล่น" : "Player",
    winRate: th ? "อัตราชนะของฮีโร่" : "Hero win rate",
    estimate: th ? "ค่าประมาณ" : "Estimate",
    guess: th ? "เดา" : "Guess",
    lowData: th ? "ข้อมูลน้อย" : "Low data",
    position: (n: number) => th ? `ตำแหน่ง ${n}` : `Pos ${n}`,
    lane: (n: number) => th ? `เลน ${laneName(n, true)}` : `Lane ${laneName(n, false)}`,
    source: {
      draft: th ? "ฮีโร่: ดราฟต์" : "Hero: draft",
      names: th ? "ชื่อ: อ่านจากจอในเครื่อง" : "Names: local screen read",
      namesOff: th ? "ชื่อผู้เล่น: ยังไม่มีในรุ่นนี้" : "Player names: not in this build",
      stats: th ? "สถิติสาธารณะ" : "Public statistics",
      statsOff: th ? "สถิติ: ยังไม่มีข้อมูล" : "Statistics: no data",
    },
  };
}

function laneName(position: number, th: boolean): string {
  if (position === 1 || position === 5) return th ? "ปลอดภัย" : "safe";
  if (position === 2) return th ? "กลาง" : "mid";
  return th ? "ยาก" : "off";
}
