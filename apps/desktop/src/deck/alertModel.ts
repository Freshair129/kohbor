// @ts-ignore Explicit source extensions support Node's TypeScript loader.
import { liveCopy } from "../live/copy.ts";
// @ts-ignore Explicit source extensions support Node's TypeScript loader.
import type { RiskView } from "../live/types.ts";
// @ts-ignore Explicit source extensions support Node's TypeScript loader.
import type { OwnHeroViewModel } from "./ownHeroModel.ts";
// @ts-ignore Explicit source extensions support Node's TypeScript loader.
import type { DeckState } from "./deckState.ts";

export type AlertChip = { kind: "health"; label: string; critical: boolean } | { kind: "vision" | "state"; label: string };

export function deriveAlertStrip(own: OwnHeroViewModel, state: DeckState, risk: RiskView | null, locale: "th" | "en") {
  const current = (state === "live" || state === "novision") && own.snapshotAvailability === "fresh";
  const hp = current ? own.hpPercent : null;
  // <=30: kohbor-risk::rules::HP_LEVEL_TWO_THRESHOLD; <=20: catalog health_low default.
  const health = hp !== null && hp <= 30 ? {
    critical: hp <= 20,
    label: locale === "th" ? (hp <= 20 ? "เลือดต่ำ · อันตราย" : "เลือดต่ำ") : (hp <= 20 ? "Low HP · danger" : "Low HP"),
  } : null;
  const text = liveCopy(locale);
  const events = new Set([risk?.lastVisionEvent, ...(risk?.state?.events ?? [])]);
  const vision = state === "live" ? (["enemy_missing", "risk_alert"] as const)
    .filter(event => events.has(event)).map(event => text.visionEvent[event]) : [];
  const ageSeconds = own.ageMs !== null && Number.isFinite(own.ageMs) && own.ageMs >= 0 ? Math.floor(own.ageMs / 1000) : "—";
  const states = {
    stale: locale === "th" ? `ข้อมูลเกมเก่า ${ageSeconds} วิ` : `Stale game data ${ageSeconds} s`,
    novision: locale === "th" ? "ไม่มีข้อมูลการมองเห็น" : "No vision data",
    standby: locale === "th" ? "รอเกมเริ่ม" : "Waiting for the game to start",
    debrief: locale === "th" ? "จบเกมแล้ว" : "Game ended",
    live: null,
  };
  const stateText = states[state];
  const chips: AlertChip[] = [];
  if (health) chips.push({ kind: "health", ...health });
  chips.push(...vision.map(label => ({ kind: "vision" as const, label })));
  if (stateText) chips.push({ kind: "state", label: stateText });
  return { health, vision, stateText, chips };
}
