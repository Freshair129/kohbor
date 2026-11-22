// UI-013 show-app hotkey: renderer contract, key capture and copy. Pure module (no Tauri imports).
import type { Locale } from "./copy";

export interface ShowHotkeyState {
  enabled: boolean;
  binding: string;
  registered: boolean;
  error_code: string | null;
  dota_conflict_risk: boolean;
}

const KEYS = ["enabled", "binding", "registered", "error_code", "dota_conflict_risk"] as const;

/** Parse the whole host reply. Any missing, extra or mistyped field makes the state unknown. */
export function parseShowHotkey(value: unknown): ShowHotkeyState | null {
  if (typeof value !== "object" || value === null || Array.isArray(value)) return null;
  const row = value as Record<string, unknown>;
  const actual = Object.keys(row);
  if (actual.length !== KEYS.length || !KEYS.every((key) => Object.hasOwn(row, key))) return null;
  if (typeof row.enabled !== "boolean" || typeof row.registered !== "boolean"
    || typeof row.dota_conflict_risk !== "boolean") return null;
  if (typeof row.binding !== "string" || row.binding.length === 0) return null;
  if (row.error_code !== null && typeof row.error_code !== "string") return null;
  return {
    enabled: row.enabled,
    binding: row.binding,
    registered: row.registered,
    error_code: row.error_code,
    dota_conflict_risk: row.dota_conflict_risk,
  };
}

export interface KeyPress {
  ctrlKey: boolean;
  altKey: boolean;
  shiftKey: boolean;
  metaKey: boolean;
  code: string;
}

export type CaptureResult =
  | { kind: "binding"; label: string }
  | { kind: "pending" }
  | { kind: "cancel" }
  | { kind: "invalid" };

/**
 * Turn one keydown into a binding label in the host's canonical order (Ctrl, Alt, Shift, Win).
 * Modifier-only presses keep waiting; Escape cancels; keys outside A–Z, 0–9, F1–F12 are invalid.
 * The host validates again (needs Ctrl, Alt or Win; reserved keys) and has the final word.
 */
export function captureKey(press: KeyPress): CaptureResult {
  const { code } = press;
  if (code === "Escape") return { kind: "cancel" };
  if (/^(Control|Alt|Shift|Meta|OS)(Left|Right)?$/.test(code)) return { kind: "pending" };
  let key: string | null = null;
  const letter = /^Key([A-Z])$/.exec(code);
  const digit = /^Digit([0-9])$/.exec(code);
  const fn = /^F([1-9]|1[0-2])$/.exec(code);
  if (letter) key = letter[1] ?? null;
  else if (digit) key = digit[1] ?? null;
  else if (fn) key = code;
  if (!key) return { kind: "invalid" };
  if (!press.ctrlKey && !press.altKey && !press.metaKey) return { kind: "invalid" };
  const parts: string[] = [];
  if (press.ctrlKey) parts.push("Ctrl");
  if (press.altKey) parts.push("Alt");
  if (press.shiftKey) parts.push("Shift");
  if (press.metaKey) parts.push("Win");
  parts.push(key);
  return { kind: "binding", label: parts.join("+") };
}

export const SHOW_HOTKEY_COPY = {
  th: {
    title: "ปุ่มลัดเรียกหน้าต่าง Kohbor",
    toggle: "เปิดใช้ปุ่มลัดเรียก/ซ่อนหน้าต่าง",
    help: "กดครั้งแรกเพื่อเรียกหน้าต่างขึ้นมา กดอีกครั้งเพื่อซ่อนและกลับไปที่เกม Kohbor ไม่ส่งปุ่มใด ๆ เข้าเกม",
    current: "ปุ่มปัจจุบัน",
    change: "เปลี่ยนปุ่ม",
    capturing: "กดปุ่มที่ต้องการ (Esc เพื่อยกเลิก)",
    reset: "คืนค่าเริ่มต้น (Ctrl+Alt+X)",
    registered: "พร้อมใช้งาน",
    notRegistered: "ยังใช้งานไม่ได้",
    off: "ปิดอยู่",
    unknown: "ไม่ทราบสถานะ",
    dotaRisk: "ปุ่ม Alt+ตัวอักษร อาจเป็นปุ่ม self-cast ใน Dota เมื่อลงทะเบียนแล้ว Dota จะไม่ได้รับปุ่มนี้",
    fullscreen: "รองรับ Dota แบบ borderless หรือ windowed เท่านั้น ถ้าใช้ exclusive fullscreen เกมอาจย่อลงเมื่อหน้าต่างขึ้นมา",
    invalidKey: "ใช้ได้เฉพาะ A–Z, 0–9, F1–F12 ร่วมกับ Ctrl, Alt หรือ Win",
    loadFailed: "โหลดการตั้งค่าปุ่มลัดไม่สำเร็จ",
    saveFailed: "ใช้ปุ่มนี้ไม่ได้ จึงคงปุ่มเดิมไว้",
  },
  en: {
    title: "Show-app hotkey",
    toggle: "Use a hotkey to show or hide the window",
    help: "Press once to bring the window up, press again to hide it and return to the game. Kohbor sends no input to the game.",
    current: "Current key",
    change: "Change key",
    capturing: "Press the new key (Esc to cancel)",
    reset: "Reset to default (Ctrl+Alt+X)",
    registered: "Ready",
    notRegistered: "Not active",
    off: "Off",
    unknown: "Unknown",
    dotaRisk: "Alt+letter may be a Dota self-cast key. Once registered, Dota no longer receives it.",
    fullscreen: "Supported over borderless or windowed Dota only. In exclusive fullscreen the game may minimize when the window comes up.",
    invalidKey: "Use A–Z, 0–9 or F1–F12 together with Ctrl, Alt or Win.",
    loadFailed: "Could not load the hotkey setting.",
    saveFailed: "That key could not be used; the previous key is kept.",
  },
} as const satisfies Record<Locale, Record<string, string>>;

const ERROR_COPY: Record<Locale, Record<string, string>> = {
  th: {
    SHOW_HOTKEY_RESERVED: "ปุ่มนี้ถูกใช้โดย Kohbor หรือ Windows อยู่แล้ว",
    SHOW_HOTKEY_NEEDS_MODIFIER: "ต้องมี Ctrl, Alt หรือ Win",
    SHOW_HOTKEY_REGISTER_FAILED: "โปรแกรมอื่นใช้ปุ่มนี้อยู่ หรือ Windows ไม่อนุญาต",
    SHELL_PREFS_WRITE_FAILED: "บันทึกการตั้งค่าไม่สำเร็จ",
  },
  en: {
    SHOW_HOTKEY_RESERVED: "Kohbor or Windows already uses this key.",
    SHOW_HOTKEY_NEEDS_MODIFIER: "Ctrl, Alt or Win is required.",
    SHOW_HOTKEY_REGISTER_FAILED: "Another program holds this key, or Windows refused it.",
    SHELL_PREFS_WRITE_FAILED: "The setting could not be saved.",
  },
};

/** Explain a host error code; the raw code stays visible so nothing is hidden. */
export function describeError(code: string, locale: Locale): string {
  const base = code.split(":")[0] ?? code;
  const text = ERROR_COPY[locale][base];
  return text ? `${text} (${code})` : code;
}
