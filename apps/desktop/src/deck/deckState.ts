// @ts-ignore Node's built-in TypeScript loader requires the explicit source extension.
import type { CaptureStatus } from "../live/types.ts";
// @ts-ignore Node's built-in TypeScript loader requires the explicit source extension.
import { phaseReasonText } from "../live/captureRecovery.ts";
// @ts-ignore Node's built-in TypeScript loader requires the explicit source extension.
import type { FieldAvailability, RuntimeStatus } from "../types.ts";
// @ts-ignore Node's built-in TypeScript loader requires the explicit source extension.
import type { Locale, Phase } from "../copy.ts";

export type DeckState = "standby" | "live" | "novision" | "stale" | "debrief";

export interface DeckStateInput {
  runtime: Pick<RuntimeStatus, "session" | "intake"> | null;
  snapshotAvailability: FieldAvailability | null;
  capture: Pick<CaptureStatus, "phase" | "paused" | "phase_reason" | "phase_detail"> | null;
  /** A failed runtime poll means the last fresh-looking reply can no longer be called current. */
  hostLost?: boolean;
}

export interface DeckStateView {
  state: DeckState;
  labelTh: string;
  labelEn: string;
  reason: { th: string; en: string };
}

const LABELS: Record<DeckState, { th: string; en: string }> = {
  standby: { th: "รอข้อมูล", en: "Standby" },
  live: { th: "สด", en: "Live" },
  novision: { th: "ไม่มีภาพ", en: "No vision" },
  stale: { th: "ข้อมูลเก่า", en: "Stale" },
  debrief: { th: "หลังเกม", en: "Debrief" },
};

function view(state: DeckState, reason: { th: string; en: string }): DeckStateView {
  const label = LABELS[state];
  return { state, labelTh: label.th, labelEn: label.en, reason };
}

function standbyReason(phase: Phase | null, intakeState: RuntimeStatus["intake"]["state"] | null) {
  if (phase === "none" || phase === "draft" || phase === "unsupported") {
    return {
      th: "ยังไม่มีข้อมูลเกมที่ใช้แสดงตัวคุณ",
      en: "No game data is available for your player yet",
    };
  }
  if (intakeState !== "receiving") {
    return {
      th: "รอข้อมูลจากเกม — ตรวจการเชื่อมต่อ GSI",
      en: "Waiting for game data — check the GSI connection",
    };
  }
  return {
    th: "ยังไม่มีข้อมูลตัวคุณที่ยืนยันความสดได้",
    en: "No fresh own-player snapshot is available",
  };
}

function staleReason(hostLost: boolean) {
  return hostLost
    ? {
        th: "ระบบหลักไม่ตอบ — ค่าที่เห็นเป็นข้อมูลล่าสุดที่ได้รับ",
        en: "The host is not responding — values shown are the last received",
      }
    : {
        th: "ข้อมูลเกมเก่า — ค่าตัวคุณเป็นข้อมูลล่าสุดที่ได้รับ",
        en: "Game data is stale — own-player values are the last received",
      };
}

function captureReason(capture: DeckStateInput["capture"]): { th: string; en: string } {
  if (!capture) {
    return { th: "ยังไม่ได้รับสถานะการจับภาพ", en: "Capture status has not arrived" };
  }

  const th = phaseReasonText(capture.phase, capture.phase_reason, "th", capture.phase_detail);
  const en = phaseReasonText(capture.phase, capture.phase_reason, "en", capture.phase_detail);
  if (th && en) return { th, en };

  switch (capture.phase) {
    case "idle":
      return { th: "รอเกม — ยังไม่ได้รับเหตุผลจากระบบหลัก", en: "Waiting — the host has not supplied a reason" };
    case "calibrating":
      return { th: "กำลังตรวจตำแหน่งแผนที่ — ยังไม่แสดงภาพสด", en: "Checking map position — no live picture yet" };
    case "stale":
      return { th: "ภาพแผนที่เก่า — กำลังรอภาพใหม่", en: "Minimap image is stale — waiting for a new frame" };
    case "coverage_unavailable":
      return { th: "ไม่มีข้อมูลการมองเห็นจากภาพ", en: "No vision coverage is available from the capture" };
    case "lost":
      return { th: "การจับภาพขาดหาย — กำลังลองใหม่", en: "Capture was lost — retrying" };
    case "unsupported":
      return { th: "การจับภาพไม่พร้อมใช้งาน", en: "Capture is unavailable" };
    case "unknown":
      return { th: "ยังยืนยันสถานะภาพไม่ได้", en: "Capture state cannot be confirmed" };
    case "live":
      if (capture.paused === true || capture.phase_reason === "paused") {
        return { th: "หยุดจับภาพ", en: "Capture is paused" };
      }
      if (capture.phase_reason === "user_invalidated") {
        return {
          th: 'คุณทำเครื่องหมายว่าแผนที่ไม่ถูกต้อง — กด "ลองใหม่" เพื่อจับภาพต่อ',
          en: 'You marked the map as wrong — press "Retry" to resume capture',
        };
      }
      return { th: "ยังไม่มีภาพที่ใช้ได้", en: "No usable picture is available" };
  }
}

/** Derive the five top-level deck states from current host, snapshot and capture inputs. */
export function deriveDeckState(input: DeckStateInput): DeckStateView {
  const phase = input.runtime?.session.phase ?? null;
  const intakeState = input.runtime?.intake.state ?? null;
  const hostLost = input.hostLost === true;
  const waiting = input.capture?.phase === "idle";

  // A completed match remains a debrief even if every live source has gone away.
  if (phase === "post_game") {
    return view("debrief", {
      th: "จบเกมแล้ว — แสดงผลล่าสุดเมื่อยังมีข้อมูลตัวคุณ",
      en: "Match ended — show the latest result when an own snapshot remains",
    });
  }

  // Explicit no-game phases do not reuse a retained snapshot as current match data.
  if (phase === null || phase === "none" || phase === "draft" || phase === "unsupported") {
    return view("standby", waiting ? captureReason(input.capture) : standbyReason(phase, intakeState));
  }

  // Old data wins over capture state, including a capture that still reports live.
  if (input.snapshotAvailability === "stale" || (hostLost && input.snapshotAvailability === "fresh")) {
    return view("stale", staleReason(hostLost));
  }

  if (intakeState !== "receiving" || input.snapshotAvailability !== "fresh") {
    return view("standby", waiting ? captureReason(input.capture) : standbyReason(phase, intakeState));
  }

  const capture = input.capture;
  if (waiting) return view("standby", captureReason(capture));
  if (capture?.phase === "live" && capture.paused !== true && capture.phase_reason !== "user_invalidated") {
    return view("live", {
      th: "ข้อมูลตัวคุณสดและได้รับภาพแผนที่",
      en: "Own-player data is fresh and minimap capture is live",
    });
  }

  return view("novision", captureReason(capture));
}

export function deckStateReasonForLocale(state: DeckStateView, locale: Locale): string {
  return locale === "th" ? state.reason.th : state.reason.en;
}
