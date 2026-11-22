// @ts-ignore Node's built-in TypeScript loader requires the source extension.
import { displayHeroName } from "./heroName.ts";

export const DRAFT_WIDTH = 1504;
export const DRAFT_HEIGHT = 125;
const FRAME_BYTES = DRAFT_WIDTH * DRAFT_HEIGHT * 4;
const SLOT_COUNT = 10;
const REASONS = {
  identified: ["Identified", "ระบุฮีโร่ได้"],
  empty_slot: ["Empty slot", "ช่องว่าง"],
  low_contrast: ["Portrait contrast too low", "ภาพฮีโร่มืดหรือไม่ชัด"],
  low_confidence: ["No confident match", "จับคู่ฮีโร่ได้ไม่มั่นใจ"],
  references_unavailable: ["Local portraits unavailable", "อ่านภาพอ้างอิงจากไฟล์เกมไม่ได้"],
  awaiting_frame: ["Awaiting draft frame", "รอภาพดราฟต์"],
} as const;
type Reason = keyof typeof REASONS;
type Team = "radiant" | "dire";
type Slot = { slot: number; team: Team; hero_id: string | null; confidence: number; reason: Reason };

/** One profile field: value or null, how sure, when (unix ms), which reader, why unknown. */
export interface RosterField<T> {
  value: T | null;
  confidence: number;
  observed_at_ms: number | null;
  source: string;
  reason: string | null;
}
export type LaneRole = "safe_lane" | "mid_lane" | "off_lane" | "support" | "hard_support";
export interface PlayerColour { name: string; rgb: [number, number, number] }
export interface SlotProfile {
  slot: number;
  side: Team;
  index: number;
  hero: RosterField<string>;
  player_colour: RosterField<PlayerColour>;
  lane_role: RosterField<LaneRole>;
  player_name: RosterField<string>;
}
/** Per-slot profiles for the current match (DEC-DRAFT-BAR revision 2). Memory only. */
export interface SlotRoster { slots: SlotProfile[]; own_slot: RosterField<number> }

export interface DraftBarFrame {
  active: boolean;
  detail: string | null;
  slots: Slot[];
  bgra: Uint8Array | null;
  roster: SlotRoster | null;
}

const LANE_ROLES: readonly LaneRole[] = ["safe_lane", "mid_lane", "off_lane", "support", "hard_support"];

function rosterField<T>(input: unknown, valid: (value: unknown) => value is T): RosterField<T> | null {
  if (!record(input) || typeof input.source !== "string" ||
      typeof input.confidence !== "number" || !Number.isFinite(input.confidence) ||
      input.confidence < 0 || input.confidence > 1 ||
      (input.reason !== null && typeof input.reason !== "string") ||
      (input.observed_at_ms !== null && (typeof input.observed_at_ms !== "number" || !Number.isFinite(input.observed_at_ms))) ||
      (input.value !== null && !valid(input.value))) return null;
  return {
    value: (input.value ?? null) as T | null, confidence: input.confidence,
    observed_at_ms: input.observed_at_ms as number | null, source: input.source, reason: input.reason as string | null,
  };
}

const isHero = (value: unknown): value is string => typeof value === "string" && /^npc_dota_hero_[a-z0-9_]{1,64}$/.test(value);
const isName = (value: unknown): value is string => typeof value === "string";
const isLane = (value: unknown): value is LaneRole => typeof value === "string" && (LANE_ROLES as readonly string[]).includes(value);
const isSlotIndex = (value: unknown): value is number => Number.isInteger(value) && (value as number) >= 0 && (value as number) < SLOT_COUNT;
function isColour(value: unknown): value is PlayerColour {
  return record(value) && typeof value.name === "string" && Array.isArray(value.rgb) && value.rgb.length === 3 &&
    value.rgb.every((channel) => Number.isInteger(channel) && channel >= 0 && channel <= 255);
}

/** A malformed roster is dropped on its own; the strip and 5b slots still render. */
export function parseSlotRoster(input: unknown): SlotRoster | null {
  if (!record(input) || !Array.isArray(input.slots) || input.slots.length !== SLOT_COUNT) return null;
  const ownSlot = rosterField(input.own_slot, isSlotIndex);
  if (!ownSlot) return null;
  const slots: SlotProfile[] = [];
  for (const [index, slot] of input.slots.entries()) {
    const side = index < 5 ? "radiant" : "dire";
    if (!record(slot) || slot.slot !== index || slot.side !== side || slot.index !== index % 5) return null;
    const hero = rosterField(slot.hero, isHero);
    const colour = rosterField(slot.player_colour, isColour);
    const lane = rosterField(slot.lane_role, isLane);
    const name = rosterField(slot.player_name, isName);
    if (!hero || !colour || !lane || !name) return null;
    slots.push({ slot: index, side, index: index % 5, hero, player_colour: colour, lane_role: lane, player_name: name });
  }
  return { slots, own_slot: ownSlot };
}

function record(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

export function parseDraftBarPacket(input: ArrayBuffer): DraftBarFrame | null {
  if (!(input instanceof ArrayBuffer) || input.byteLength < 4) return null;
  const metadataLength = new DataView(input).getUint32(0, true);
  // The roster (DEC-DRAFT-BAR revision 2) makes the metadata about 5 KB.
  if (metadataLength > 16384 || metadataLength + 4 > input.byteLength) return null;
  const pixelsAt = 4 + metadataLength;
  let meta: unknown;
  try {
    meta = JSON.parse(new TextDecoder().decode(new Uint8Array(input, 4, metadataLength)));
  } catch {
    return null;
  }
  if (!record(meta) || typeof meta.active !== "boolean" || typeof meta.has_frame !== "boolean" ||
      (meta.detail !== null && typeof meta.detail !== "string") ||
      !Array.isArray(meta.slots) || meta.slots.length !== SLOT_COUNT ||
      (!meta.active && meta.has_frame) ||
      input.byteLength !== pixelsAt + (meta.has_frame ? FRAME_BYTES : 0)) return null;

  const slots: Slot[] = [];
  for (const [index, slot] of meta.slots.entries()) {
    const team = index < 5 ? "radiant" : "dire";
    if (!record(slot) || slot.slot !== index || slot.team !== team ||
        typeof slot.reason !== "string" || !Object.hasOwn(REASONS, slot.reason) ||
        typeof slot.confidence !== "number" || !Number.isFinite(slot.confidence) ||
        slot.confidence < 0 || slot.confidence > 1) return null;
    const hero = slot.hero_id;
    if (slot.reason === "identified" ?
      (!meta.has_frame || typeof hero !== "string" || !/^npc_dota_hero_[a-z0-9_]{1,64}$/.test(hero)) :
      hero !== null) return null;
    slots.push({
      slot: index, team, hero_id: typeof hero === "string" ? hero : null,
      confidence: slot.confidence, reason: slot.reason as Reason,
    });
  }
  return {
    active: meta.active,
    detail: meta.detail,
    slots,
    bgra: meta.has_frame ? new Uint8Array(input, pixelsAt) : null,
    roster: meta.active ? parseSlotRoster(meta.roster) : null,
  };
}

export function draftBarPresentation(frame: DraftBarFrame | null, hostLost: boolean, locale: "en" | "th") {
  if (hostLost || !frame?.active) return null;
  return {
    detail: frame.detail,
    slots: frame.slots.map((slot) => {
      const confidence = Math.round(slot.confidence * 100);
      const reason = REASONS[slot.reason][locale === "th" ? 1 : 0];
      return {
        column: slot.slot + (slot.team === "radiant" ? 1 : 2),
        team: slot.team,
        text: displayHeroName(slot.hero_id)?.name ?? "?",
        title: `${reason} · ${confidence}%`,
      };
    }),
  };
}
