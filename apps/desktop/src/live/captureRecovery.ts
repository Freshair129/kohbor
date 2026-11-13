// @ts-ignore Node's built-in TypeScript loader requires the explicit source extension.
import type { CapturePhase } from "./types.ts";
// @ts-ignore Node's built-in TypeScript loader requires the explicit source extension.
import type { LiveLocale } from "./copy.ts";

export const UNSUPPORTED_CAPTURE_REASONS = [
  "user_invalidated", "no_game_window", "crop_out_of_bounds", "unsupported_resolution", "capture_disabled",
] as const;

const REASON_TEXT = {
  th: {
    user_invalidated: 'คุณทำเครื่องหมายว่าแผนที่ไม่ถูกต้อง — กด "ลองใหม่" เพื่อจับภาพต่อ',
    no_game_window: "ไม่พบหน้าต่าง Dota 2 ที่มองเห็นได้ — เปิดเกมในโหมด Borderless",
    crop_out_of_bounds: 'กรอบที่ตั้งไว้ออกนอกจอ — กด "รีเซ็ต" หรือปรับกรอบ',
    unsupported_resolution: "จอต้องเป็น 1920x1080 ที่สเกล 100% (รุ่นนี้)",
    capture_unavailable: "จับภาพจากหน้าจอไม่ได้ในขณะนี้ — ระบบกำลังลองใหม่",
    capture_disabled: "ปิดการจับภาพด้วย KOHBOR_CAPTURE=0",
    paused: "หยุดจับภาพ",
    reopening: "กำลังเปิดการจับภาพใหม่",
    access_lost: "การเชื่อมต่อการจับภาพขาดหาย — กำลังลองใหม่",
    calibrating: "กำลังตรวจตำแหน่งแผนที่ — ยังไม่แสดงภาพสด",
  },
  en: {
    user_invalidated: 'You marked the map as wrong — press "Retry" to resume capture',
    no_game_window: "No visible Dota 2 window found — open the game in Borderless mode",
    crop_out_of_bounds: 'The configured crop is outside the screen — press "Reset" or adjust the crop',
    unsupported_resolution: "Display must be 1920x1080 at 100% scaling (this build)",
    capture_unavailable: "Screen capture is unavailable right now — retrying",
    capture_disabled: "Capture disabled by KOHBOR_CAPTURE=0",
    paused: "Capture paused",
    reopening: "Reopening capture",
    access_lost: "Capture access lost — retrying",
    calibrating: "Checking map position — live picture is hidden",
  },
} as const;

export function phaseReasonText(
  phase: CapturePhase | null,
  reason: string | null,
  locale: LiveLocale,
  detail: string | null = null,
): string | null {
  const suffix = detail ? ` (${detail})` : "";
  if (reason === null) {
    if (phase !== "unsupported") return null;
    return locale === "th" ? "จับภาพไม่ได้ (ไม่ทราบเหตุผล)" : "Capture unavailable (reason unknown)";
  }
  const messages = REASON_TEXT[locale];
  if (Object.hasOwn(messages, reason)) {
    return `${messages[reason as keyof typeof messages]}${reason === "capture_unavailable" ? suffix : ""}`;
  }
  return locale === "th" ? `จับภาพไม่ได้ (เหตุผล: ${reason})` : `Capture unavailable (reason: ${reason})`;
}

export const INVALIDATION_CONFIRM_MS = 4_000;

export function isInvalidationArmed(armedUntil: number | null, now: number): boolean {
  return armedUntil !== null && now < armedUntil;
}

export function advanceInvalidation(armedUntil: number | null, now: number): { armedUntil: number | null; invoke: boolean } {
  return isInvalidationArmed(armedUntil, now)
    ? { armedUntil: null, invoke: true }
    : { armedUntil: now + INVALIDATION_CONFIRM_MS, invoke: false };
}
