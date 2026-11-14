// @ts-ignore Explicit source extensions support Node's TypeScript loader.
import { liveCopy } from "../live/copy.ts";
// @ts-ignore Explicit source extensions support Node's TypeScript loader.
import type { RiskView } from "../live/types.ts";
// @ts-ignore Explicit source extensions support Node's TypeScript loader.
import type { OwnHeroViewModel } from "./ownHeroModel.ts";
// @ts-ignore Explicit source extensions support Node's TypeScript loader.
import type { DeckState } from "./deckState.ts";

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
  const states = {
    stale: locale === "th" ? "ข้อมูลเก่า" : "Stale game data",
    novision: locale === "th" ? "ไม่มีภาพสด" : "No live vision",
    standby: locale === "th" ? "รอข้อมูลเกม" : "Waiting for game data",
    debrief: locale === "th" ? "ผลล่าสุด · หลังเกม" : "Latest result · post game",
    live: null,
  };
  return { health, vision, stateText: states[state] };
}
