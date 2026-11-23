// Hero cards for the second-monitor Live mode (UI-017; owner request 2026-11-23).
// Each card: portrait, full name, HP bar, mana bar and four skill squares (three skills + ultimate).
// A value appears only when an admitted source exists; everything else is null and renders "—".
// @ts-ignore Node's built-in TypeScript loader requires the explicit source extension.
import { displayHeroName } from "../deck/heroName.ts";
// @ts-ignore Node's built-in TypeScript loader requires the explicit source extension.
import type { AbilityTile } from "../deck/loadoutModel.ts";
// @ts-ignore Node's built-in TypeScript loader requires the explicit source extension.
import type { DraftRosterMemory, Team } from "./secondMonitorModel.ts";

/**
 * State of an ally's ultimate dot on the in-game top bar (proposed DEC-TOPBAR-CV, owned by the top-bar session):
 * green = ult ready and enough mana, blue = off cooldown but not enough mana, dark = still on cooldown or no mana.
 * This branch only accepts the input; it never reads the screen.
 */
export const ULT_DOT_STATES = ["ready", "ready_no_mana", "not_ready"] as const;
export type UltDotState = (typeof ULT_DOT_STATES)[number];

/** Input `topbar.ally_ult_state`, keyed by draft slot 0..9. Absent or null means unknown. */
export interface TopbarInput { ally_ult_state: Partial<Record<number, UltDotState | null>> }

/** Consented teammate sharing (PEER-003, PEER-010; extension M6), keyed by draft slot. Not on this branch. */
export interface PeerSkill { cooldown: number | null; ready: boolean | null }
export interface PeerVitals {
  hpPercent: number | null;
  manaPercent: number | null;
  /** Up to three regular skills then the ultimate, in game order. */
  skills: (PeerSkill | null)[];
  ultimate: PeerSkill | null;
}
export type PeerInput = Partial<Record<number, PeerVitals | null>>;

export type SkillSource = "gsi" | "peer" | "topbar" | null;
export type SkillState = "cooldown" | "ready" | "ready_no_mana" | "not_ready" | "unknown";
export interface SkillSquare {
  ultimate: boolean;
  state: SkillState;
  /** Seconds shown only from GSI or peer sharing; the top-bar dot never has a number. */
  seconds: number | null;
  source: SkillSource;
}

export type CardSide = "own" | "ally" | "enemy";
export interface HeroCard {
  key: string;
  side: CardSide;
  /** Draft slot 0..9 when known. */
  slot: number | null;
  heroId: string | null;
  heroName: string | null;
  nameSource: "gsi" | "draft" | null;
  hpPercent: number | null;
  manaPercent: number | null;
  vitalsSource: "gsi" | "peer" | null;
  skills: [SkillSquare, SkillSquare, SkillSquare, SkillSquare];
}

const unknownSkill = (ultimate: boolean): SkillSquare => ({ ultimate, state: "unknown", seconds: null, source: null });
const unknownSkills = (): HeroCard["skills"] => [unknownSkill(false), unknownSkill(false), unknownSkill(false), unknownSkill(true)];
const percent = (value: number | null | undefined): number | null =>
  typeof value === "number" && Number.isFinite(value) ? Math.max(0, Math.min(100, value)) : null;
const positive = (value: number | null | undefined): number | null =>
  typeof value === "number" && Number.isFinite(value) && value > 0 ? value : null;

function teamSlots(team: Team): number[] {
  return team === "radiant" ? [0, 1, 2, 3, 4] : [5, 6, 7, 8, 9];
}

function fromTile(tile: AbilityTile | undefined, ultimate: boolean): SkillSquare {
  if (!tile) return unknownSkill(ultimate);
  if (tile.status === "cooldown" && tile.cooldown !== null) return { ultimate, state: "cooldown", seconds: tile.cooldown, source: "gsi" };
  if (tile.status === "ready") return { ultimate, state: "ready", seconds: null, source: "gsi" };
  return unknownSkill(ultimate);
}

/** Own hero skills from GSI tiles: the first three non-ultimate active abilities, then the ultimate. */
export function ownSkillSquares(tiles: AbilityTile[] | null): HeroCard["skills"] {
  if (!tiles) return unknownSkills();
  const regular = tiles.filter(tile => tile.ultimate === false);
  const ult = tiles.find(tile => tile.ultimate === true);
  return [fromTile(regular[0], false), fromTile(regular[1], false), fromTile(regular[2], false), fromTile(ult, true)];
}

export function ownHeroCard(input: {
  heroId: string | null; hpPercent: number | null; manaPercent: number | null; tiles: AbilityTile[] | null; slot?: number | null;
}): HeroCard {
  const name = displayHeroName(input.heroId)?.name ?? null;
  const hp = percent(input.hpPercent);
  const mana = percent(input.manaPercent);
  return {
    key: "own", side: "own", slot: input.slot ?? null, heroId: name ? input.heroId : null, heroName: name,
    nameSource: name ? "gsi" : null, hpPercent: hp, manaPercent: mana,
    vitalsSource: hp !== null || mana !== null ? "gsi" : null, skills: ownSkillSquares(input.tiles),
  };
}

function fromPeer(skill: PeerSkill | null | undefined, ultimate: boolean): SkillSquare {
  if (!skill) return unknownSkill(ultimate);
  const seconds = positive(skill.cooldown);
  if (seconds !== null) return { ultimate, state: "cooldown", seconds, source: "peer" };
  if (skill.ready === true) return { ultimate, state: "ready", seconds: null, source: "peer" };
  return unknownSkill(ultimate);
}

function fromDot(state: UltDotState | null | undefined): SkillSquare {
  return state && (ULT_DOT_STATES as readonly string[]).includes(state)
    ? { ultimate: true, state, seconds: null, source: "topbar" }
    : unknownSkill(true);
}

/**
 * Ally card. Peer sharing gives HP, mana and cooldowns. Without it, HP, mana and the three skills stay unknown and
 * the ult square shows only the top-bar dot state (no number). Peer data wins over the dot when both exist.
 */
export function allyHeroCard(key: string, slot: number | null, heroId: string | null, peer: PeerVitals | null, dot: UltDotState | null): HeroCard {
  const name = displayHeroName(heroId)?.name ?? null;
  if (slot === null) {
    return { key, side: "ally", slot: null, heroId: null, heroName: null, nameSource: null, hpPercent: null, manaPercent: null, vitalsSource: null, skills: unknownSkills() };
  }
  const hp = percent(peer?.hpPercent);
  const mana = percent(peer?.manaPercent);
  const peerUlt = fromPeer(peer?.ultimate, true);
  return {
    key, side: "ally", slot, heroId: name ? heroId : null, heroName: name, nameSource: name ? "draft" : null,
    hpPercent: hp, manaPercent: mana, vitalsSource: hp !== null || mana !== null ? "peer" : null,
    skills: [fromPeer(peer?.skills[0], false), fromPeer(peer?.skills[1], false), fromPeer(peer?.skills[2], false),
      peerUlt.source === "peer" ? peerUlt : fromDot(dot)],
  };
}

/**
 * Four ally cards, own hero excluded. If the own hero cannot be matched to a draft slot, the four cards stay unknown
 * (no slot, so no dot either) rather than risk repeating the own hero or binding a dot to the wrong ally.
 */
export function allyHeroCards(input: {
  memory: DraftRosterMemory; epoch: number | null; ownTeam: Team | null; ownHeroId: string | null;
  peer?: PeerInput | null; topbar?: TopbarInput | null;
}): HeroCard[] {
  const { memory, epoch, ownTeam, ownHeroId } = input;
  const current = memory.epoch !== null && memory.epoch === epoch;
  let slots: (number | null)[] = [null, null, null, null];
  if (current && ownTeam && ownHeroId) {
    const team = teamSlots(ownTeam);
    const ownSlot = team.find(slot => memory.slots[slot] === ownHeroId);
    if (ownSlot !== undefined) slots = team.filter(slot => slot !== ownSlot);
  }
  return slots.map((slot, index) => allyHeroCard(`A${index + 1}`, slot, slot === null ? null : memory.slots[slot] ?? null,
    slot === null ? null : input.peer?.[slot] ?? null, slot === null ? null : input.topbar?.ally_ult_state[slot] ?? null));
}

/** The own hero's draft slot, when it can be matched in same-session draft memory. */
export function ownDraftSlot(memory: DraftRosterMemory, epoch: number | null, ownTeam: Team | null, ownHeroId: string | null): number | null {
  if (memory.epoch === null || memory.epoch !== epoch || !ownHeroId) return null;
  const candidates = ownTeam ? teamSlots(ownTeam) : [0, 1, 2, 3, 4, 5, 6, 7, 8, 9];
  const matches = candidates.filter(slot => memory.slots[slot] === ownHeroId);
  return matches.length === 1 ? matches[0]! : null;
}

/**
 * Five enemy cards. Identity comes only from same-session Draft Bar memory; HP, mana and skills have no admitted
 * source, so they stay unknown. Without the own team the enemy side is not known and the names stay unknown.
 */
export function enemyHeroCards(memory: DraftRosterMemory, epoch: number | null, ownTeam: Team | null): HeroCard[] {
  const current = memory.epoch !== null && memory.epoch === epoch;
  const enemy: Team | null = ownTeam === null ? null : ownTeam === "radiant" ? "dire" : "radiant";
  return [0, 1, 2, 3, 4].map((index): HeroCard => {
    const slot: number | null = enemy ? teamSlots(enemy)[index] ?? null : null;
    const heroId = current && slot !== null ? memory.slots[slot] ?? null : null;
    const name = displayHeroName(heroId)?.name ?? null;
    return {
      key: `E${index + 1}`, side: "enemy", slot, heroId: name ? heroId : null, heroName: name,
      nameSource: name ? "draft" : null, hpPercent: null, manaPercent: null, vitalsSource: null, skills: unknownSkills(),
    };
  });
}

export function cardCopy(locale: "th" | "en") {
  const th = locale === "th";
  return {
    ourTeam: th ? "ทีมเรา" : "Our team",
    enemyTeam: th ? "ฝั่งตรงข้าม" : "Enemy team",
    radiant: th ? "เรเดียนท์" : "Radiant",
    dire: th ? "ไดร์" : "Dire",
    enemyMark: th ? "ศัตรู" : "Enemy",
    you: th ? "คุณ" : "You",
    hp: "HP",
    mana: th ? "มานา" : "Mana",
    ult: th ? "อัลติ" : "Ult",
    skill: (n: number) => th ? `สกิล ${n}` : `Skill ${n}`,
    state: th
      ? { cooldown: "คูลดาวน์", ready: "พร้อม", ready_no_mana: "พร้อมแต่มานาไม่พอ", not_ready: "ยังไม่พร้อม", unknown: "ไม่ทราบ" }
      : { cooldown: "Cooldown", ready: "Ready", ready_no_mana: "Ready, not enough mana", not_ready: "Not ready", unknown: "Unknown" },
    source: {
      gsi: "GSI",
      draft: th ? "ชื่อ: ดราฟต์" : "Name: draft",
      peer: th ? "แชร์จากเพื่อน" : "Teammate sharing",
      topbar: th ? "อัลติ: แถบบน" : "Ult: top bar",
      peerPending: th ? "HP/มานา/สกิล: ต้องแชร์จากเพื่อน" : "HP/mana/skills: needs teammate sharing",
      topbarPending: th ? "อัลติ: รอการอ่านแถบบน" : "Ult: awaiting top-bar read",
      enemyNone: th ? "HP/มานา/สกิล: ไม่มีแหล่งข้อมูล" : "HP/mana/skills: no source",
    },
  };
}
