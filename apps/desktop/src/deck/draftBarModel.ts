// @ts-ignore Node's built-in TypeScript loader requires the source extension.
import { displayHeroName } from "./heroName.ts";

export const DRAFT_WIDTH = 1494;
export const DRAFT_HEIGHT = 119;
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

export interface DraftBarFrame {
  active: boolean;
  detail: string | null;
  slots: Slot[];
  bgra: Uint8Array | null;
}

function record(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

export function parseDraftBarPacket(input: ArrayBuffer): DraftBarFrame | null {
  if (!(input instanceof ArrayBuffer) || input.byteLength < 4) return null;
  const metadataLength = new DataView(input).getUint32(0, true);
  if (metadataLength > 4096 || metadataLength + 4 > input.byteLength) return null;
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
