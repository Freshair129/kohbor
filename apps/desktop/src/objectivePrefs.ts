export type ObjectivePrefKey =
  | "water_rune"
  | "power_rune"
  | "bounty_rune"
  | "wisdom_rune"
  | "tormentor"
  | "diagnostic_map_keys";

export type ObjectiveId = Exclude<ObjectivePrefKey, "diagnostic_map_keys">;

export interface ObjectivePreference {
  enabled: boolean;
  lead_s: number[];
}

export interface ObjectivePrefs {
  schema_version: 2;
  objectives: Record<ObjectiveId, ObjectivePreference>;
  diagnostic_map_keys: boolean;
}

export type LeadTimeValidationError = "whole_seconds" | "range" | "duplicate" | null;

export const OBJECTIVE_PREF_KEYS: readonly ObjectiveId[] = [
  "water_rune", "power_rune", "bounty_rune", "wisdom_rune", "tormentor",
];

function record(value: unknown): Record<string, unknown> | null {
  return typeof value === "object" && value !== null && !Array.isArray(value)
    ? value as Record<string, unknown>
    : null;
}

function exactKeys(value: Record<string, unknown>, allowed: readonly string[]): boolean {
  const keys = Object.keys(value);
  return keys.length === allowed.length && keys.every((key) => allowed.includes(key));
}

function validLeadTimes(value: unknown): value is number[] {
  if (!Array.isArray(value) || value.length > 2) return false;
  const seen = new Set<number>();
  for (const seconds of value) {
    if (typeof seconds !== "number" || !Number.isInteger(seconds) || seconds < 5 || seconds > 60 || seen.has(seconds)) return false;
    seen.add(seconds);
  }
  return true;
}

export function parseObjectivePrefs(value: unknown): ObjectivePrefs | null {
  const root = record(value);
  if (!root || !exactKeys(root, ["schema_version", "objectives", "diagnostic_map_keys"])
    || root.schema_version !== 2 || typeof root.diagnostic_map_keys !== "boolean") return null;
  const objectives = record(root.objectives);
  if (!objectives || !exactKeys(objectives, OBJECTIVE_PREF_KEYS)) return null;

  const parsed = {} as Record<ObjectiveId, ObjectivePreference>;
  for (const id of OBJECTIVE_PREF_KEYS) {
    const setting = record(objectives[id]);
    if (!setting || !exactKeys(setting, ["enabled", "lead_s"])
      || typeof setting.enabled !== "boolean" || !validLeadTimes(setting.lead_s)) return null;
    parsed[id] = { enabled: setting.enabled, lead_s: [...setting.lead_s] };
  }
  return {
    schema_version: 2,
    objectives: parsed,
    diagnostic_map_keys: root.diagnostic_map_keys,
  };
}

export function validateLeadTimeInputs(inputs: readonly string[]): { values: number[]; error: LeadTimeValidationError } {
  const filled = inputs.map((value) => value.trim()).filter((value) => value.length > 0);
  if (filled.length > 2) return { values: [], error: "range" };
  const values: number[] = [];
  for (const value of filled) {
    if (!/^\d+$/u.test(value)) return { values: [], error: "whole_seconds" };
    const seconds = Number(value);
    if (!Number.isSafeInteger(seconds)) return { values: [], error: "whole_seconds" };
    if (seconds < 5 || seconds > 60) return { values: [], error: "range" };
    if (values.includes(seconds)) return { values: [], error: "duplicate" };
    values.push(seconds);
  }
  return { values, error: null };
}
