import type { Locale } from "../copy";

export interface MatchRecapCopy {
  title: string;
  matchTitle(index: number): string;
  empty: string;
  noPlayed: string;
  dropped(count: number): string;
  played: string;
  tier(value: number): string;
  variant(value: string): string;
  clip(value: string): string;
  selection(value: string): string;
  end: {
    finished: string;
    cut: string;
    failed: string;
    unknown: string;
  };
  clear: string;
  confirmPrompt: string;
  confirm: string;
  cancel: string;
  clearing: string;
  clearFailed: string;
  unreadable: string;
  skipped(count: number): string;
  footnote: string;
}

const COPY: Record<Locale, MatchRecapCopy> = {
  th: {
    title: "แมตช์ล่าสุด",
    matchTitle: (index) => (index === 0 ? "แมตช์ล่าสุด" : `แมตช์ก่อนหน้า ${index}`),
    empty: "ยังไม่มีแมตช์ที่บันทึกไว้",
    noPlayed: "ไม่มีเสียงประกาศที่บันทึกว่าเล่นแล้วในแมตช์นี้",
    dropped: (count) => `มี ${count} เหตุการณ์ที่ไม่ได้เล่น`,
    played: "เล่นแล้ว",
    tier: (value) => `ระดับ ${value}`,
    variant: (value) => `แบบ ${value}`,
    clip: (value) => `คลิป ${value}`,
    selection: (value) => `การเลือก ${value}`,
    end: {
      finished: "เล่นจบ",
      cut: "ถูกตัดก่อนจบ",
      failed: "เล่นไม่สำเร็จ",
      unknown: "ยังไม่มีข้อมูลการจบ",
    },
    clear: "ล้างประวัติ",
    confirmPrompt: "ล้างประวัติแมตช์ที่บันทึกไว้ทั้งหมดหรือไม่",
    confirm: "ยืนยันล้าง",
    cancel: "ยกเลิก",
    clearing: "กำลังล้าง…",
    clearFailed: "ล้างประวัติไม่สำเร็จ",
    unreadable: "อ่านประวัติแมตช์ไม่ได้",
    skipped: (count) => `ข้ามรายการที่อ่านไม่ได้ ${count} รายการ`,
    footnote: "รายการที่ถูกดรอปไม่แสดงเป็นเสียงที่เล่นแล้ว; “เล่นแล้ว” หมายถึงส่งเสียงให้ระบบเล่น ไม่ได้ยืนยันว่าคุณได้ยิน และ “ถูกตัด” หมายถึงเสียงหยุดก่อนจบ",
  },
  en: {
    title: "Recent matches",
    matchTitle: (index) => (index === 0 ? "Latest match" : `Previous match ${index}`),
    empty: "No matches have been recorded yet.",
    noPlayed: "No announcer events were recorded as played in this match.",
    dropped: (count) => `${count} events were dropped`,
    played: "Played",
    tier: (value) => `Tier ${value}`,
    variant: (value) => `Variant ${value}`,
    clip: (value) => `Clip ${value}`,
    selection: (value) => `Selection ${value}`,
    end: {
      finished: "Finished",
      cut: "Cut short",
      failed: "Playback failed",
      unknown: "End state not recorded",
    },
    clear: "Clear history",
    confirmPrompt: "Clear all saved match history?",
    confirm: "Confirm clear",
    cancel: "Cancel",
    clearing: "Clearing…",
    clearFailed: "Could not clear match history.",
    unreadable: "Match history could not be read.",
    skipped: (count) => `Skipped ${count} unreadable records`,
    footnote: "Dropped events are not shown as played. “Played” means audio was handed to the playback system; it does not prove you heard it. “Cut short” means playback stopped before finishing.",
  },
};

export function matchRecapCopy(locale: Locale): MatchRecapCopy {
  return COPY[locale];
}
