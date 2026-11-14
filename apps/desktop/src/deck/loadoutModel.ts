// @ts-ignore Explicit extensions support Node's TypeScript loader.
import type { FieldAvailability, ItemSlot, OwnSnapshot } from "../types.ts";
// @ts-ignore Explicit extensions support Node's TypeScript loader.
import { ITEM_SLOTS } from "../ownLoadout.ts";
// @ts-ignore Explicit extensions support Node's TypeScript loader.
import { displayItemName } from "./itemName.ts";
// @ts-ignore Explicit extensions support Node's TypeScript loader.
import { displayAbilityName } from "./abilityName.ts";

export interface ItemTile {
  slot: ItemSlot; kind: "item" | "empty" | "unknown"; name: string | null; rawId: string | null;
  cooldown: number | null; charges: number | null;
}
export interface AbilityTile {
  slot: number; name: string; rawId: string; level: number | null; pips: boolean[] | null; ultimate: boolean | null;
  status: "cooldown" | "ready" | "unknown"; cooldown: number | null;
}
const positive = (value: number | null): number | null => typeof value === "number" && Number.isSafeInteger(value) && value > 0 ? value : null;

export function deriveLoadoutViewModel(snapshot: OwnSnapshot | null, hostLost = false) {
  const fields = snapshot?.own?.fields;
  const items = fields?.items; const abilities = fields?.abilities;
  const itemsAvailability: FieldAvailability = items?.availability ?? "unknown";
  const abilitiesAvailability: FieldAvailability = abilities?.availability ?? "unknown";
  const current = snapshot?.availability === "fresh" && !hostLost;
  const sourceAge = snapshot?.source.age_ms;
  const ageMs = !hostLost && (snapshot?.availability === "fresh" || snapshot?.availability === "stale")
    && typeof sourceAge === "number" && Number.isFinite(sourceAge) && sourceAge >= 0 ? sourceAge : null;
  const stale = (availability: FieldAvailability) => hostLost || snapshot?.availability === "stale" || availability === "stale";
  let slots: ItemTile[] | null = null;
  if (current && itemsAvailability === "fresh" && Array.isArray(items?.value)) {
    const observed = new Map(items.value.map(entry => [entry.slot, entry]));
    slots = ITEM_SLOTS.map(slot => {
      const entry = observed.get(slot);
      const display = displayItemName(entry?.name ?? null);
      return { slot, kind: !entry ? "unknown" : entry.name === null ? "empty" : "item", name: display?.name ?? null,
        rawId: display?.rawId ?? null, cooldown: display && entry ? positive(entry.cooldown) : null,
        charges: display && entry ? positive(entry.charges) : null };
    });
  }
  let tiles: AbilityTile[] | null = null;
  if (current && abilitiesAvailability === "fresh" && Array.isArray(abilities?.value)) {
    tiles = abilities.value.filter(entry => entry.passive === false).sort((a, b) => a.slot - b.slot).map(entry => {
      const display = displayAbilityName(entry.name, fields?.hero_id.value);
      const cooldown = positive(entry.cooldown);
      return { slot: entry.slot, name: display?.name ?? entry.name, rawId: entry.name, level: entry.level,
        pips: entry.level === null || entry.ultimate === null ? null
          : Array.from({ length: entry.ultimate ? 6 : 4 }, (_, index) => index < entry.level!),
        ultimate: entry.ultimate, cooldown, status: cooldown !== null ? "cooldown" : entry.can_cast === true ? "ready" : "unknown" };
    });
  }
  const legacy = snapshot?.schema_version === "2.0.0" || snapshot?.schema_version === "2.1.0";
  const otherFresh = fields && Object.entries(fields).some(([key, field]) => key !== "items" && key !== "abilities" && field?.availability === "fresh");
  const missingBlock = itemsAvailability === "unknown" || abilitiesAvailability === "unknown";
  return {
    items: { availability: itemsAvailability, stale: stale(itemsAvailability), ageMs, slots },
    abilities: { availability: abilitiesAvailability, stale: stale(abilitiesAvailability), ageMs, tiles },
    setupHint: Boolean(fields && missingBlock && (legacy || (current && otherFresh))),
  };
}
export type LoadoutViewModel = ReturnType<typeof deriveLoadoutViewModel>;

export function abilityStatusText(tile: AbilityTile, locale: "th" | "en"): string {
  return tile.status === "cooldown" && tile.cooldown !== null ? `${tile.cooldown}s`
    : tile.status === "ready" ? locale === "th" ? "พร้อม" : "Ready" : "—";
}
