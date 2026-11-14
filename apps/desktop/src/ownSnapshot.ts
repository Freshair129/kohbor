// @ts-ignore Explicit extensions support Node's TypeScript loader.
import type { Field, FieldAvailability, OwnSnapshot } from "./types.ts";

export const OWN_STAT_FIELDS = ["gold", "kills", "deaths", "assists", "last_hits", "denies", "gpm", "xpm"] as const;
export type OwnStatKey = typeof OWN_STAT_FIELDS[number];
const BASE_FIELDS = ["hero_id", "level", "hp", "max_hp", "mana_percent", "alive", "respawn_seconds"] as const;
const AVAILABILITY = ["fresh", "stale", "unknown", "invalid", "unsupported"] as const;
const bounds: Record<string, [number, number]> = {
  level: [1, 30], hp: [0, 100000], max_hp: [1, 100000], mana_percent: [0, 100], respawn_seconds: [0, 600],
  gold: [0, 1000000], kills: [0, 10000], deaths: [0, 10000], assists: [0, 10000],
  last_hits: [0, 10000], denies: [0, 10000], gpm: [0, 10000], xpm: [0, 10000],
};
function object(input: unknown): Record<string, unknown> | null {
  return input !== null && typeof input === "object" && !Array.isArray(input) ? input as Record<string, unknown> : null;
}
function closed(input: Record<string, unknown>, keys: readonly string[]): boolean {
  return Object.keys(input).length === keys.length && keys.every(key => Object.hasOwn(input, key));
}
function integer(value: unknown, min: number, max = Number.MAX_SAFE_INTEGER): value is number {
  return typeof value === "number" && Number.isSafeInteger(value) && value >= min && value <= max;
}
function availability(value: unknown): value is FieldAvailability {
  return typeof value === "string" && (AVAILABILITY as readonly string[]).includes(value);
}
function field(input: unknown, name: string): Field<unknown> | null {
  const f = object(input);
  if (!f || !closed(f, ["value", "availability"]) || !availability(f.availability)) return null;
  if (f.availability === "fresh" || f.availability === "stale") {
    const range = bounds[name];
    const valid = name === "hero_id"
      ? typeof f.value === "string" && /^npc_dota_hero_[a-z0-9_]{1,100}$/.test(f.value)
      : name === "alive" ? typeof f.value === "boolean" : range && integer(f.value, range[0], range[1]);
    if (!valid) return null;
  } else if (f.value !== null) return null;
  return { value: f.value, availability: f.availability };
}

/** Validate the renderer boundary; legacy missing counters are explicitly unknown. */
export function parseOwnSnapshot(input: unknown): OwnSnapshot | null {
  const s = object(input);
  if (!s || !closed(s, ["schema_version", "session_epoch", "availability", "source", "own"])) return null;
  if (s.schema_version !== "2.0.0" && s.schema_version !== "2.1.0") return null;
  if (!availability(s.availability) || (s.session_epoch !== null && !integer(s.session_epoch, 1))) return null;
  const source = object(s.source);
  if (!source || !closed(source, ["kind", "generation", "sequence", "age_ms"]) || source.kind !== "gsi") return null;
  if (!integer(source.generation, 1) || (source.sequence !== null && !integer(source.sequence, 1))
    || (source.age_ms !== null && !integer(source.age_ms, 0))) return null;
  const currentOrOld = s.availability === "fresh" || s.availability === "stale";
  let own: OwnSnapshot["own"] = null;
  if (currentOrOld) {
    if (s.session_epoch === null || source.sequence === null || source.age_ms === null) return null;
    if (s.availability === "fresh" ? (source.age_ms as number) > 2000 : (source.age_ms as number) < 2001) return null;
    const rawOwn = object(s.own);
    if (!rawOwn || !closed(rawOwn, ["identity_revision", "team", "fields"]) || !integer(rawOwn.identity_revision, 1)) return null;
    if (rawOwn.team !== null && rawOwn.team !== "radiant" && rawOwn.team !== "dire") return null;
    const rawFields = object(rawOwn.fields);
    const names = [...BASE_FIELDS, ...OWN_STAT_FIELDS];
    if (!rawFields || Object.keys(rawFields).some(key => !names.includes(key as typeof names[number]))) return null;
    const parsed: Record<string, Field<unknown>> = {};
    for (const name of names) {
      const f = s.schema_version === "2.0.0" && OWN_STAT_FIELDS.includes(name as OwnStatKey) && !Object.hasOwn(rawFields, name)
        ? { value: null, availability: "unknown" as const } : field(rawFields[name], name);
      if (!f || (s.availability === "fresh" && f.availability === "stale") || (s.availability === "stale" && f.availability === "fresh")) return null;
      parsed[name] = f;
    }
    const hp = parsed.hp?.value; const maxHp = parsed.max_hp?.value;
    if (typeof hp === "number" && typeof maxHp === "number" && hp > maxHp) return null;
    own = { identity_revision: rawOwn.identity_revision, team: rawOwn.team, fields: parsed as unknown as NonNullable<OwnSnapshot["own"]>["fields"] };
  } else if (s.own !== null) return null;
  return {
    schema_version: s.schema_version, session_epoch: s.session_epoch as number | null, availability: s.availability,
    source: { kind: "gsi", generation: source.generation, sequence: source.sequence as number | null, age_ms: source.age_ms as number | null }, own,
  };
}
