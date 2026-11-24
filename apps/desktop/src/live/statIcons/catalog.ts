export const STAT_KEYS = [
  "str", "agi", "int", "dmg", "reg", "arm", "mr", "amp", "as", "ms", "ev", "sr", "ls", "gpm", "xpm",
] as const;

export type StatKey = typeof STAT_KEYS[number];
export type StatIconId =
  | "primary_strength" | "primary_agility" | "primary_intelligence" | "primary_universal"
  | "damage" | "health_regen" | "mana_regen" | "armor" | "magic_resist" | "spell_amp"
  | "attack_speed" | "move_speed" | "evasion" | "status_resistance" | "lifesteal" | "gold_coin" | "xp";

export interface StatIconDefinition {
  label: string;
  accessibleLabel: string;
  iconIds: readonly StatIconId[];
}

/** Shared labels and game-asset keys for Live and Replay stat cells. */
export const STAT_ICON_CATALOG: Readonly<Record<StatKey, StatIconDefinition>> = {
  str: { label: "STR", accessibleLabel: "STR — พลัง (Strength)", iconIds: ["primary_strength"] },
  agi: { label: "AGI", accessibleLabel: "AGI — ความว่องไว (Agility)", iconIds: ["primary_agility"] },
  int: { label: "INT", accessibleLabel: "INT — สติปัญญา (Intelligence)", iconIds: ["primary_intelligence"] },
  dmg: { label: "DMG", accessibleLabel: "DMG — พลังโจมตี (Damage)", iconIds: ["damage"] },
  reg: { label: "REG", accessibleLabel: "REG — ฟื้นฟูพลังชีวิตและมานา (Health and Mana Regeneration)", iconIds: ["health_regen", "mana_regen"] },
  arm: { label: "ARM", accessibleLabel: "ARM — เกราะ (Armor)", iconIds: ["armor"] },
  mr: { label: "MR", accessibleLabel: "MR — ต้านทานเวท (Magic Resistance)", iconIds: ["magic_resist"] },
  amp: { label: "AMP", accessibleLabel: "AMP — เพิ่มพลังเวท (Spell Amplification)", iconIds: ["spell_amp"] },
  as: { label: "AS", accessibleLabel: "AS — ความเร็วโจมตี (Attack Speed)", iconIds: ["attack_speed"] },
  ms: { label: "MS", accessibleLabel: "MS — ความเร็วเคลื่อนที่ (Movement Speed)", iconIds: ["move_speed"] },
  ev: { label: "EV", accessibleLabel: "EV — หลบหลีก (Evasion)", iconIds: ["evasion"] },
  sr: { label: "SR", accessibleLabel: "SR — ต้านทานสถานะ (Status Resistance)", iconIds: ["status_resistance"] },
  ls: { label: "LS", accessibleLabel: "LS — ดูดเลือด (Lifesteal)", iconIds: ["lifesteal"] },
  gpm: { label: "GPM", accessibleLabel: "GPM — ทองต่อนาที (Gold per Minute)", iconIds: ["gold_coin"] },
  xpm: { label: "XPM", accessibleLabel: "XPM — ค่าประสบการณ์ต่อนาที (Experience per Minute)", iconIds: ["xp"] },
};

/** Used when a hero view needs to identify Universal as its primary attribute. */
export const PRIMARY_ATTRIBUTE_ICON_IDS = {
  strength: "primary_strength",
  agility: "primary_agility",
  intelligence: "primary_intelligence",
  universal: "primary_universal",
} as const satisfies Record<string, StatIconId>;

export function statIconDefinition(key: StatKey): StatIconDefinition {
  return STAT_ICON_CATALOG[key];
}
