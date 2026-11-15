/** All HUD text lives here so the language can be switched back to Thai in one place. */
const english = {
  stripLabel: "Game info strip",
  meterTitle: "Risk",
  meterLabels: {
    none: "No signal",
    unseen: "Out of sight",
    risk: "Risky",
    danger: "Danger",
    unavailable: "Data not ready",
  },
  chipLabels: {
    kda: "K/D/A",
    gold: "Gold",
    gpm: "GPM",
    nw: "NW",
    xpm: "XPM",
  },
  staleFlag: "stale",
  syntheticBadge: "SYNTHETIC PREVIEW",
} as const;

const thai = {
  stripLabel: "แถบข้อมูลเกม",
  meterTitle: "ความเสี่ยง",
  meterLabels: {
    none: "ยังไม่มีสัญญาณเสี่ยง",
    unseen: "มองไม่เห็น",
    risk: "เสี่ยง",
    danger: "อันตราย",
    unavailable: "ข้อมูลยังไม่พร้อม",
  },
  chipLabels: {
    kda: "K/D/A",
    gold: "ทอง",
    gpm: "GPM",
    nw: "NW",
    xpm: "XPM",
  },
  staleFlag: "ข้อมูลเก่า",
  syntheticBadge: "ตัวอย่างสังเคราะห์",
} as const;

export type HudCopy = { [K in keyof typeof english]: (typeof english)[K] extends string ? string : { [P in keyof (typeof english)[K]]: string } };
export const hudCopyTables: { en: HudCopy; th: HudCopy } = { en: english, th: thai };

/** The language shown by the HUD (English first, the owner's order for dev.14). */
export const HUD_LANGUAGE: "en" | "th" = "en";
export const hudCopy: HudCopy = hudCopyTables[HUD_LANGUAGE];
