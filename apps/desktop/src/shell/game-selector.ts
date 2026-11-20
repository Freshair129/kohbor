export type DotaConnection = "receiving" | "waiting" | "unknown";

export interface GameMenuItem {
  id: string;
  name: string;
  initials: string;
  selectable: boolean;
  focusable: boolean;
  status: "connected" | "waiting" | "unknown" | "unsupported";
}

const UNSUPPORTED_GAMES = [
  ["league-of-legends", "League of Legends", "LoL"],
  ["overwatch", "Overwatch", "OW"],
  ["valorant", "VALORANT", "VAL"],
  ["apex-legends", "Apex Legends", "APX"],
  ["counter-strike-2", "Counter-Strike 2", "CS2"],
] as const;

export function gameMenuItems(connection: DotaConnection): GameMenuItem[] {
  return [
    { id: "dota2", name: "Dota 2", initials: "D2", selectable: true, focusable: true,
      status: connection === "receiving" ? "connected" : connection === "waiting" ? "waiting" : "unknown" },
    ...UNSUPPORTED_GAMES.map(([id, name, initials]) => ({
      id, name, initials, selectable: false, focusable: true, status: "unsupported" as const,
    })),
  ];
}

export function unsupportedGameExplanation(name: string, locale: "th" | "en"): string {
  return locale === "th"
    ? `Kohbor ยังไม่รองรับ ${name} และยังไม่มีกำหนดวันเปิด ตอนนี้ใช้ได้กับ Dota 2 เท่านั้น`
    : `Kohbor does not support ${name} yet; Dota 2 is the only supported game and no release date is set.`;
}

export function gameStatusLabel(status: GameMenuItem["status"], locale: "th" | "en"): string {
  if (status === "connected") return locale === "th" ? "GSI: รับข้อมูล" : "GSI: receiving";
  if (status === "waiting") return locale === "th" ? "GSI: รอข้อมูล" : "GSI: waiting";
  if (status === "unsupported") return locale === "th" ? "ยังไม่รองรับ" : "Not supported";
  return locale === "th" ? "GSI: ไม่ทราบ" : "GSI: unknown";
}
