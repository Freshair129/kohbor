// Thai copy for the closed error codes (contracts/error-catalog.json message keys)
// and for the honest data states (contracts/design-tokens.json state_encoding).

export const OWNED_CONFIG_FILE = "gamestate_integration_kohbor.cfg";

const REASON_TH: Record<string, string> = {
  SOURCE_UNAVAILABLE: "ยังไม่ได้ตั้งค่าการเชื่อมต่อเกม",
  SOURCE_STALE: "ข้อมูลจากเกมเก่าเกิน 2 วินาที",
  SOURCE_INVALID: "ข้อมูลที่ได้รับไม่ใช่ข้อมูลจาก Dota 2",
  OWNER_UNVERIFIED: "ยืนยันผู้เล่นไม่ได้ — ล็อกอิน Steam แล้วเปิด Dota 2 ใหม่",
  METRIC_UNAVAILABLE: "วัดค่าไม่ได้ในขณะนี้ — การจำกัดทรัพยากรยังทำงานอยู่",
  GSI_GAME_NOT_FOUND: "ไม่พบ Dota 2 ในคลัง Steam ของเครื่องนี้ หรือพบมากกว่าหนึ่งที่",
  GSI_CONFIG_CONFLICT: "ไฟล์ตั้งค่าถูกแก้จากภายนอก — ยังไม่เขียนทับ",
  GSI_CONFIG_WRITE_FAILED: "เขียนไฟล์ตั้งค่าในโฟลเดอร์ Dota 2 ไม่ได้",
  GSI_PORT_UNAVAILABLE: "พอร์ตที่ตั้งไว้ถูกโปรแกรมอื่นใช้อยู่",
  GSI_PORT_EXHAUSTED: "พอร์ต 3000–3010 ถูกใช้หมด",
  GSI_SECRET_UNAVAILABLE: "เข้าถึงที่เก็บความลับของ Windows ไม่ได้",
  MEDIA_UNMAPPED: "แพ็กที่เลือกยังไม่มีเสียงเตือนสำคัญที่ผ่านการตรวจ",
  CAPABILITY_UNAVAILABLE: "ยังไม่มีในบิลด์นี้",
};

/** Localized reason. An unmapped code is shown with a plain-language lead, never alone. */
export function reason(code: string | null | undefined): string {
  if (!code) return "";
  return REASON_TH[code] ?? `เกิดข้อผิดพลาดที่ยังไม่มีคำอธิบาย (${code})`;
}

export function seconds(ms: number | null | undefined): string {
  return ms === null || ms === undefined ? "—" : `${(ms / 1000).toFixed(1)} วิ`;
}

export type Phase = "none" | "draft" | "pregame" | "live" | "post_game" | "unsupported";

export const PHASE_TH: Record<Phase, string> = {
  none: "รอเข้าเกม",
  draft: "เลือกฮีโร่",
  pregame: "ก่อนเริ่ม",
  live: "กำลังเล่น",
  post_game: "จบเกม",
  unsupported: "ไม่รองรับ",
};
