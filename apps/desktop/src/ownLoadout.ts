// @ts-ignore Explicit extensions support Node's TypeScript loader.
import type { Field, FieldAvailability, ItemSlot, OwnAbility, OwnItem } from "./types.ts";

export const ITEM_SLOTS: readonly ItemSlot[] = ["inventory0", "inventory1", "inventory2", "inventory3", "inventory4", "inventory5",
  "backpack0", "backpack1", "backpack2", "teleport", "neutral"];
const states: readonly FieldAvailability[] = ["fresh", "stale", "unknown", "invalid", "unsupported"];
// Keep scalar bounds aligned with kohbor-gsi/src/admission.rs.
const MAX_COOLDOWN_SECONDS = 3600;
const MAX_CHARGES = 999;
const object = (value: unknown): value is Record<string, unknown> => value !== null && typeof value === "object" && !Array.isArray(value);
const id = (value: unknown): value is string => typeof value === "string" && /^[a-z0-9_]{1,64}$/.test(value);
function nullableInteger(value: unknown, max: number): number | null | undefined {
  if (value === null) return null;
  return typeof value === "number" && Number.isSafeInteger(value) && value >= 0 && value <= max ? value : undefined;
}
const nullableBoolean = (value: unknown): boolean | null | undefined => value === null ? null : typeof value === "boolean" ? value : undefined;

function item(value: unknown): OwnItem | null {
  if (!object(value) || !ITEM_SLOTS.includes(value.slot as ItemSlot) || !(value.name === null || id(value.name))) return null;
  const cooldown = nullableInteger(value.cooldown, MAX_COOLDOWN_SECONDS); const charges = nullableInteger(value.charges, MAX_CHARGES);
  const can_cast = nullableBoolean(value.can_cast); const passive = nullableBoolean(value.passive);
  if (cooldown === undefined || charges === undefined || can_cast === undefined || passive === undefined) return null;
  return { slot: value.slot as ItemSlot, name: value.name, cooldown, charges, can_cast, passive };
}
function ability(value: unknown): OwnAbility | null {
  if (!object(value) || value.slot == null || nullableInteger(value.slot, 7) === undefined || !id(value.name)) return null;
  const level = nullableInteger(value.level, 30); const cooldown = nullableInteger(value.cooldown, MAX_COOLDOWN_SECONDS);
  const ultimate = nullableBoolean(value.ultimate); const can_cast = nullableBoolean(value.can_cast);
  const passive = nullableBoolean(value.passive); const active = nullableBoolean(value.active);
  if (level === undefined || cooldown === undefined || ultimate === undefined || can_cast === undefined || passive === undefined || active === undefined) return null;
  return { slot: value.slot as number, name: value.name, level, cooldown, ultimate, can_cast, passive, active };
}

/** Invalid entries invalidate only this collection. Discard its payload so a partial
 * valid-looking list cannot masquerade as a complete inventory or ability bar. */
export function parseLoadoutField<T extends OwnItem | OwnAbility>(input: unknown, kind: "items" | "abilities", snapshotAvailability: FieldAvailability): Field<T[]> {
  const invalid: Field<T[]> = { value: null, availability: "invalid" };
  if (input === undefined) return { value: null, availability: "unknown" };
  if (!object(input) || Object.keys(input).length !== 2 || !Object.hasOwn(input, "value")
    || !states.includes(input.availability as FieldAvailability)) return invalid;
  const availability = input.availability as FieldAvailability;
  if ((snapshotAvailability === "fresh" && availability === "stale") || (snapshotAvailability === "stale" && availability === "fresh")) return invalid;
  if (input.value === null) return availability === "fresh" || availability === "stale" ? invalid : { value: null, availability };
  if ((availability !== "fresh" && availability !== "stale") || !Array.isArray(input.value)
    || input.value.length > (kind === "items" ? ITEM_SLOTS.length : 8)) return invalid;
  const entries: T[] = []; const slots = new Set<ItemSlot | number>();
  for (const raw of input.value) {
    const entry = kind === "items" ? item(raw) : ability(raw);
    if (!entry || slots.has(entry.slot)) return invalid;
    slots.add(entry.slot); entries.push(entry as T);
  }
  return { value: entries, availability };
}
