// Second-monitor Live mode (UI-016, UI-017; DEC-LIVE-SECOND-MONITOR).
// Pure layout and read-model helpers. Every value either comes from an admitted source or is null ("—").
// @ts-ignore Node's built-in TypeScript loader requires the explicit source extension.
import { displayHeroName } from "../deck/heroName.ts";
// @ts-ignore Node's built-in TypeScript loader requires the explicit source extension.
import type { DraftBarFrame } from "../deck/draftBarModel.ts";
// @ts-ignore Node's built-in TypeScript loader requires the explicit source extension.
import type { RiskState } from "./types.ts";

/** Where monitor 2 sits relative to the main monitor. "off" keeps today's centred deck. */
export const SECOND_MONITOR_SIDES = ["off", "left", "right"] as const;
export type SecondMonitorSide = (typeof SECOND_MONITOR_SIDES)[number];
export const DEFAULT_SECOND_MONITOR_SIDE: SecondMonitorSide = "off";
export const SECOND_MONITOR_STORAGE_KEY = "kohbor.live-second-monitor";

export function isSecondMonitorSide(value: unknown): value is SecondMonitorSide {
  return typeof value === "string" && (SECOND_MONITOR_SIDES as readonly string[]).includes(value);
}

export function readSecondMonitorSide(storage: Pick<Storage, "getItem">): SecondMonitorSide {
  try {
    const saved = storage.getItem(SECOND_MONITOR_STORAGE_KEY);
    return isSecondMonitorSide(saved) ? saved : DEFAULT_SECOND_MONITOR_SIDE;
  } catch {
    return DEFAULT_SECOND_MONITOR_SIDE;
  }
}

/** Returns the requested side even if storage is unavailable, so the UI still changes in memory. */
export function persistSecondMonitorSide(storage: Pick<Storage, "setItem">, side: SecondMonitorSide): SecondMonitorSide {
  try { storage.setItem(SECOND_MONITOR_STORAGE_KEY, side); } catch { /* In-memory display still changes. */ }
  return side;
}

/** Match phases in which the second-monitor deck replaces the centred deck: hero selection through the game. */
export const SECOND_MONITOR_MATCH_PHASES = ["draft", "pregame", "live"] as const;

/**
 * True while a match runs (draft, pregame or live) and the host still answers. Unknown is not a match: a null phase
 * (no status read yet) or a lost host keeps the centred deck. A single failed poll does not reach here as null, because
 * the runtime keeps the last status until the host counts as lost.
 */
export function isMatchRunning(phase: string | null | undefined, hostLost: boolean): boolean {
  return !hostLost && typeof phase === "string" && (SECOND_MONITOR_MATCH_PHASES as readonly string[]).includes(phase);
}

/**
 * The side to pin on the Live page, or null for the centred deck (UI-016, owner report on build 3815c73, 2026-11-23).
 * The stored side is only a preference: outside a match (none, post_game, unsupported, unknown or host lost) the Live
 * page shows the centred deck exactly as with "off", and the side selector stays reachable there.
 */
export function pinnedSecondMonitorSide(input: {
  side: SecondMonitorSide; onLivePage: boolean; phase: string | null | undefined; hostLost: boolean;
}): Exclude<SecondMonitorSide, "off"> | null {
  if (!input.onLivePage || input.side === "off") return null;
  return isMatchRunning(input.phase, input.hostLost) ? input.side : null;
}

/**
 * Monitor 2 to the RIGHT of the main monitor means its LEFT edge touches the main monitor, so the map is pinned left.
 * The rails go to the opposite (outer) edge.
 */
export function mapEdgeFor(side: Exclude<SecondMonitorSide, "off">): "left" | "right" {
  return side === "right" ? "left" : "right";
}

/** Enemies no longer on the map compared with the recent maximum seen (risk baseline). Derived; null if either is unknown. */
export function missingFromMap(state: RiskState | null): number | null {
  if (!state || state.visible === null || state.baseline === null) return null;
  return Math.max(0, state.baseline - state.visible);
}

// ---- Draft identity memory: in memory for the match only (UI-012); cleared on session change. ----
export type Team = "radiant" | "dire";
export interface RosterSlot { slot: number; team: Team; heroId: string | null }
export interface DraftRosterMemory { epoch: number | null; slots: (string | null)[] }

export function createDraftRosterMemory(): DraftRosterMemory {
  return { epoch: null, slots: Array(10).fill(null) };
}

/** Clears the memory when the session epoch changes or is unknown (match end, session change). */
export function syncDraftRosterEpoch(memory: DraftRosterMemory, epoch: number | null): DraftRosterMemory {
  if (epoch === null) return createDraftRosterMemory();
  if (memory.epoch !== epoch) return { epoch, slots: Array(10).fill(null) };
  return memory;
}

/** Keeps identified draft slots. A later unidentified read never erases an earlier identification in the same session. */
export function rememberDraftFrame(memory: DraftRosterMemory, epoch: number | null, frame: DraftBarFrame | null): DraftRosterMemory {
  const synced = syncDraftRosterEpoch(memory, epoch);
  if (epoch === null || !frame?.active) return synced;
  let changed = false;
  const slots = synced.slots.slice();
  for (const slot of frame.slots) {
    if (slot.reason === "identified" && slot.hero_id && slots[slot.slot] !== slot.hero_id) {
      slots[slot.slot] = slot.hero_id;
      changed = true;
    }
  }
  return changed ? { epoch, slots } : synced;
}

export interface RosterRow {
  key: string;
  heroName: string | null;
  /** Alive/dead has no admitted source until the in-game top-bar read exists (DEC-TOPBAR-CV, blocked on frames). */
  life: null;
}

function teamSlots(team: Team): number[] {
  return team === "radiant" ? [0, 1, 2, 3, 4] : [5, 6, 7, 8, 9];
}

/** Five stable enemy rows. Without the own team we cannot tell which draft side is the enemy, so names stay unknown. */
export function enemyRows(memory: DraftRosterMemory, epoch: number | null, ownTeam: Team | null): RosterRow[] {
  const current = memory.epoch !== null && memory.epoch === epoch;
  const enemy: Team | null = ownTeam === null ? null : ownTeam === "radiant" ? "dire" : "radiant";
  const slots = enemy ? teamSlots(enemy) : [0, 1, 2, 3, 4].map(() => -1);
  return slots.map((slot, index) => ({
    key: `E${index + 1}`,
    heroName: current && slot >= 0 ? displayHeroName(memory.slots[slot] ?? null)?.name ?? null : null,
    life: null,
  }));
}

/** The next map objective. No source exists on this branch; ALERT-008 supplies it when merged. */
export interface NextObjective { label: string; secondsUntil: number }

export function formatCountdown(seconds: number | null): string {
  if (seconds === null || !Number.isFinite(seconds) || seconds < 0) return "—";
  const whole = Math.floor(seconds);
  return `${Math.floor(whole / 60)}:${String(whole % 60).padStart(2, "0")}`;
}

export function secondMonitorCopy(locale: "th" | "en") {
  const th = locale === "th";
  return {
    sideLabel: th ? "จอ 2 อยู่" : "Monitor 2 is",
    sideOptions: th
      ? { off: "ปิด", left: "ซ้ายจอหลัก", right: "ขวาจอหลัก" }
      : { off: "Off", left: "Left of main", right: "Right of main" },
    sideHint: th ? "แผนที่จะชิดขอบที่ติดกับจอหลัก" : "The map is pinned to the edge that touches the main monitor",
    tooSmallTitle: th ? "หน้าต่างเล็กเกินไป" : "Window too small",
    tooSmallBody: th ? "โหมดจอ 2 ต้องการหน้าต่างอย่างน้อย 688×680 เพื่อแสดงแผนที่และการ์ดทีมเราครบโดยไม่ต้องเลื่อน"
      : "Second-monitor mode needs a window of at least 688×680 to show the map and all of our cards without scrolling",
    tooSmallHint: th ? "ขยายหน้าต่าง หรือปิดโหมดจอ 2" : "Enlarge the window or turn second-monitor mode off",
    slotLabel: th ? "ช่องอิสระ" : "Free panel",
    slotTitle: th ? "เหตุการณ์" : "Events",
    slotNote: th ? "ช่องอิสระ · ภายหลังรองรับเสียง AI หรือแชทกับเอเจนต์" : "Free panel · later: AI voice or agent chat",
    danger: th ? "ความเสี่ยง" : "Danger",
    missing: th ? "หายจากแผนที่" : "Missing from map",
    missingUnit: th ? "ตัว" : "",
    visible: (count: string) => th ? `เห็นอยู่ ${count}` : `${count} visible`,
    enemies: th ? "ศัตรู" : "Enemies",
    allies: th ? "ทีมเดียวกัน" : "Allies",
    nextObjective: th ? "รูนถัดไป" : "Next rune",
    objectiveNoSource: th ? "ยังไม่มีระบบรูนในรุ่นนี้" : "No rune timer in this build",
    lifeUnknown: "—",
    own: th ? "คุณ" : "You",
    level: th ? "เลเวล" : "Lv",
    alive: th ? "มีชีวิต" : "Alive",
    dead: (seconds: string) => th ? `ตาย · เกิดใหม่ ${seconds} วิ` : `Dead · respawn ${seconds} s`,
    more: th ? "ไอเท็ม สกิล ทอง KDA" : "Items, skills, gold, KDA",
    source: {
      draft: th ? "ชื่อ: ดราฟต์" : "Name: draft",
      lifePending: th ? "เป็น/ตาย: รอการอ่านแถบบน" : "Alive/dead: awaiting top-bar read",
      peerPending: th ? "HP/สกิล: ต้องแชร์จากเพื่อน (ยังไม่มีในรุ่นนี้)" : "HP/skills: needs teammate sharing (not in this build)",
      map: th ? "จากภาพแผนที่" : "From the map",
      derived: th ? "คำนวณจากภาพแผนที่" : "Derived from the map",
      risk: th ? "คำนวณ" : "Derived",
      gsi: "GSI",
      objective: th ? "นาฬิกาเกม" : "Game clock",
    },
    unmatchedOwn: th ? "ยังจับคู่ตัวคุณกับดราฟต์ไม่ได้" : "Your hero is not matched to the draft yet",
    noTeam: th ? "ยังไม่รู้ว่าคุณอยู่ฝั่งไหน" : "Your team is not known yet",
  };
}
