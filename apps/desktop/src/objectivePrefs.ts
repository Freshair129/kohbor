export type ObjectivePrefKey =
  | "water_rune"
  | "power_rune"
  | "bounty_rune"
  | "wisdom_rune"
  | "tormentor"
  | "diagnostic_map_keys";

export interface ObjectivePrefs {
  schema_version: 1;
  water_rune: boolean;
  power_rune: boolean;
  bounty_rune: boolean;
  wisdom_rune: boolean;
  tormentor: boolean;
  diagnostic_map_keys: boolean;
}

const PREF_KEYS: readonly ObjectivePrefKey[] = [
  "water_rune", "power_rune", "bounty_rune", "wisdom_rune", "tormentor", "diagnostic_map_keys",
];

export function parseObjectivePrefs(value: unknown): ObjectivePrefs | null {
  if (typeof value !== "object" || value === null || Array.isArray(value)) return null;
  const row = value as Record<string, unknown>;
  if (row.schema_version !== 1 || Object.keys(row).length !== PREF_KEYS.length + 1
    || PREF_KEYS.some((key) => typeof row[key] !== "boolean")
    || Object.keys(row).some((key) => key !== "schema_version" && !PREF_KEYS.includes(key as ObjectivePrefKey))) return null;
  return row as unknown as ObjectivePrefs;
}
