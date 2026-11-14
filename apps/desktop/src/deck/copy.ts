// @ts-ignore Node's built-in TypeScript loader requires the explicit source extension.
import type { Locale } from "../copy.ts";

export const ALLY_SLOT_IDS = ["A1", "A2", "A3", "A4"] as const;
export const ENEMY_SLOT_IDS = ["E1", "E2", "E3", "E4", "E5"] as const;

export const DECK_PLACEHOLDERS = {
  allies: { th: "— ยังไม่มีข้อมูล", en: "— no data yet" },
  allySource: { th: "เกมให้ข้อมูลเฉพาะตัวคุณ", en: "The game provides data for your hero only" },
  enemies: { th: "— ยังไม่เห็น · ต้องใช้ภาพ", en: "— not seen · needs vision" },
  emptyEvents: { th: "ยังไม่มีเหตุการณ์", en: "No events yet" },
  unavailableInVersion: { th: "ยังไม่มีข้อมูลในรุ่นนี้", en: "Data is not available in this version" },
  noPack: { th: "ไม่มีแพ็ก", en: "No pack" },
} as const;

export function deckCopy(locale: Locale) {
  const english = locale === "en";
  const language = english ? "en" : "th";
  return {
    ownHero: english ? "ฮีโร่ของคุณ · YOUR HERO" : "ฮีโร่ของคุณ",
    level: english ? "LV" : "เลเวล",
    hp: "HP",
    mana: english ? "Mana" : "มานา",
    alive: english ? "Alive" : "สถานะ",
    age: english ? "Age" : "อายุข้อมูล",
    oldData: english ? "Old data" : "ข้อมูลเก่า",
    latestResult: english ? "Latest result" : "ผลล่าสุด",
    statsHeading: english ? "More stats" : "ค่าสถานะอื่น",
    statCaption: DECK_PLACEHOLDERS.unavailableInVersion[language],
    statLabels: english
      ? ["Gold", "K / D / A", "LH / DN", "GPM / XPM", "Items", "Cooldowns"]
      : ["ทอง", "K / D / A", "LH / DN", "GPM / XPM", "ไอเท็ม", "คูลดาวน์"],
    allies: english ? "ทีมเดียวกัน · ALLIES" : "ทีมเดียวกัน",
    enemies: english ? "ศัตรู · ENEMIES" : "ศัตรู",
    eventFeed: english ? "เหตุการณ์ · EVENT FEED" : "เหตุการณ์ · EVENT FEED",
    soundPack: english ? "เสียงและแพ็ก · SOUND / PACK" : "เสียงและแพ็ก · SOUND / PACK",
    audioReadiness: english ? "Audio readiness" : "ความพร้อมเสียง",
    activePack: english ? "Active pack" : "แพ็กที่ใช้อยู่",
    mute: english ? "Mute" : "เสียง",
    muted: english ? "Muted" : "ปิดเสียง",
    unmuted: english ? "Unmuted" : "เปิดเสียง",
    unknown: "—",
    placeholder: DECK_PLACEHOLDERS,
  };
}
