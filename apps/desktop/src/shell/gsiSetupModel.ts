import type { RuntimeStatus } from "../types";

export type GsiPhase = "not_installed" | "waiting_restart" | "receiving" | "stale" | "problem" | "starting";

/** Where the Dota 2 connection (GSI) stands, from the host intake state only; unknown stays a problem/starting, never "receiving". */
export function gsiPhase(status: RuntimeStatus | null): GsiPhase | null {
  if (status === null) return null;
  switch (status.intake.state) {
    case "not_configured": return "not_installed";
    case "listening": return "waiting_restart";
    case "receiving": return "receiving";
    case "stale": return "stale";
    case "starting": return "starting";
    default: return "problem";
  }
}

/** The connection panel stays visible on the live page until data is really arriving. */
export function gsiNeedsAttention(phase: GsiPhase | null): boolean {
  return phase !== null && phase !== "receiving";
}

export function isConfigConflict(status: RuntimeStatus | null): boolean {
  return status?.intake.state === "unavailable" && status.intake.reason === "GSI_CONFIG_CONFLICT";
}

export function gsiSetupFailure(status: RuntimeStatus | null, commandFailure: string | null): string | null {
  const hostFailure = status?.intake.state === "unavailable" ? status.intake.reason : null;
  const failure = commandFailure ?? hostFailure;
  // The card already explains this reason when it offers the replacement action.
  return failure === "GSI_CONFIG_CONFLICT" && isConfigConflict(status) ? null : failure;
}

const LABELS = {
  th: {
    not_installed: "ยังไม่ได้ติดตั้งไฟล์เชื่อมต่อ Dota 2",
    waiting_restart: "ติดตั้งแล้ว — รอข้อมูล ให้ปิดแล้วเปิด Dota 2 ใหม่หนึ่งครั้ง",
    receiving: "กำลังรับข้อมูลจาก Dota 2",
    stale: "ติดตั้งแล้ว แต่ข้อมูลจากเกมหยุดมาแล้ว",
    starting: "กำลังเริ่มรับข้อมูล…",
    problem: "การเชื่อมต่อ Dota 2 มีปัญหา",
    action: "ตั้งค่า GSI / ติดตั้งไฟล์เชื่อมต่อ Dota 2",
    actionAgain: "ติดตั้งไฟล์เชื่อมต่อซ้ำ",
    replace: "แทนที่ไฟล์ตั้งค่า…",
    confirmReplace: "ยืนยันแทนที่ไฟล์",
    cancel: "ยกเลิก",
    busy: "กำลังติดตั้ง…",
    done: "ติดตั้งไฟล์เชื่อมต่อแล้ว — ปิดแล้วเปิด Dota 2 ใหม่หนึ่งครั้งเพื่อเริ่มส่งข้อมูล",
    conflict: "ไฟล์ตั้งค่าของ Kohbor ถูกแก้จากภายนอก จึงยังไม่เขียนทับ ต้องการแทนที่หรือไม่",
    title: "การเชื่อมต่อ Dota 2 (GSI)",
  },
  en: {
    not_installed: "The Dota 2 connection file is not installed",
    waiting_restart: "Installed — waiting for data. Restart Dota 2 once.",
    receiving: "Receiving data from Dota 2",
    stale: "Installed, but game data stopped arriving",
    starting: "Starting to listen…",
    problem: "The Dota 2 connection has a problem",
    action: "Set up GSI / install the Dota 2 connection file",
    actionAgain: "Reinstall the connection file",
    replace: "Replace settings file…",
    confirmReplace: "Confirm replace",
    cancel: "Cancel",
    busy: "Installing…",
    done: "Connection file installed — restart Dota 2 once so it starts sending data.",
    conflict: "Kohbor's settings file was changed externally and has not been overwritten. Replace it?",
    title: "Dota 2 connection (GSI)",
  },
} as const;

export function gsiCopy(locale: "th" | "en") {
  return LABELS[locale];
}

export function gsiStateLabel(phase: GsiPhase, locale: "th" | "en"): string {
  return LABELS[locale][phase];
}
