// @ts-ignore Explicit extensions support Node's TypeScript loader.
import type { FieldAvailability, ItemSlot, OwnSnapshot } from "../types.ts";
// @ts-ignore Explicit extensions support Node's TypeScript loader.
import { ITEM_SLOTS } from "../ownLoadout.ts";
// @ts-ignore Explicit extensions support Node's TypeScript loader.
import { displayItemName } from "./itemName.ts";
// @ts-ignore Explicit extensions support Node's TypeScript loader.
import { displayAbilityName } from "./abilityName.ts";
// @ts-ignore Explicit extensions support Node's TypeScript loader.
import { formatCooldownValue } from "./cooldownModel.ts";

export interface ItemTile {
  slot: ItemSlot; kind: "item" | "empty" | "unknown"; name: string | null; rawId: string | null;
  cooldown: number | null; charges: number | null;
}
export interface AbilityTile {
  slot: number; name: string; rawId: string; level: number | null; pips: boolean[] | null; ultimate: boolean | null;
  status: "cooldown" | "ready" | "unknown"; cooldown: number | null; canCast: boolean | null;
}
export interface FreshLoadoutCache {
  contextKey: string;
  items: ItemTile[] | null;
  abilities: AbilityTile[] | null;
}
const positive = (value: number | null): number | null => typeof value === "number" && Number.isFinite(value) && value > 0 ? value : null;

const MAIN_ITEM_SLOTS: readonly ItemSlot[] = ["inventory0", "inventory1", "inventory2", "inventory3", "inventory4", "inventory5"];
const BACKPACK_SLOTS: readonly ItemSlot[] = ["backpack0", "backpack1", "backpack2"];

export function groupItemSlots(slots: readonly ItemTile[]) {
  const bySlot = new Map(slots.map(tile => [tile.slot, tile]));
  const collect = (ordered: readonly ItemSlot[]) => ordered.flatMap(slot => {
    const tile = bySlot.get(slot);
    return tile ? [tile] : [];
  });
  return {
    main: collect(MAIN_ITEM_SLOTS),
    backpack: collect(BACKPACK_SLOTS),
    teleport: bySlot.get("teleport") ?? null,
    neutral: bySlot.get("neutral") ?? null,
  };
}

export function splitAbilityRows(tiles: readonly AbilityTile[]) {
  const extras: AbilityTile[] = [];
  const hero: AbilityTile[] = [];
  for (const tile of tiles) {
    if (tile.rawId.startsWith("plus_") || tile.rawId.startsWith("seasonal_")) extras.push(tile);
    else hero.push(tile);
  }
  return { hero, extras };
}

/** Keep previously fresh values only while the session, source generation, identity revision, and hero agree. */
export function retainFreshLoadout(model: LoadoutViewModel, previous: FreshLoadoutCache | null) {
  const { generation, sessionEpoch, identityRevision, heroId } = model.source;
  const contextKey = generation !== null && sessionEpoch !== null && identityRevision !== null && heroId !== null
    ? JSON.stringify([generation, sessionEpoch, identityRevision, heroId]) : null;
  const prior = contextKey !== null && previous?.contextKey === contextKey ? previous : null;
  if (contextKey === null) return { cache: null, items: model.items.stale ? null : model.items.slots,
    abilities: model.abilities.stale ? null : model.abilities.tiles };

  const cache: FreshLoadoutCache = {
    contextKey,
    items: model.items.stale ? prior?.items ?? null : model.items.slots,
    abilities: model.abilities.stale ? prior?.abilities ?? null : model.abilities.tiles,
  };
  return {
    cache,
    items: model.items.slots ?? (model.items.stale ? cache.items : null),
    abilities: model.abilities.tiles ?? (model.abilities.stale ? cache.abilities : null),
  };
}

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
  if (current && itemsAvailability === "fresh" && Array.isArray(items?.value) && items.value.length > 0) {
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
      return { slot: entry.slot, name: display?.name ?? entry.name, rawId: entry.name, level: entry.level, canCast: entry.can_cast,
        pips: entry.level === null || entry.ultimate === null ? null
          : Array.from({ length: entry.ultimate ? 6 : 4 }, (_, index) => index < entry.level!),
        ultimate: entry.ultimate, cooldown, status: cooldown !== null ? "cooldown" : entry.can_cast === true ? "ready" : "unknown" };
    });
  }
  const legacy = snapshot?.schema_version === "2.0.0" || snapshot?.schema_version === "2.1.0";
  const otherFresh = fields && Object.entries(fields).some(([key, field]) => key !== "items" && key !== "abilities" && field?.availability === "fresh");
  const missingBlock = itemsAvailability === "unknown" || abilitiesAvailability === "unknown";
  return {
    source: {
      generation: snapshot?.source.generation ?? null,
      sequence: snapshot?.source.sequence ?? null,
      ageMs: sourceAge !== undefined && sourceAge !== null && Number.isFinite(sourceAge) && sourceAge >= 0 ? sourceAge : null,
      sessionEpoch: snapshot?.session_epoch ?? null,
      identityRevision: snapshot?.own?.identity_revision ?? null,
      heroId: fields?.hero_id?.value ?? null,
    },
    items: { availability: itemsAvailability, stale: stale(itemsAvailability), ageMs, slots },
    abilities: { availability: abilitiesAvailability, stale: stale(abilitiesAvailability), ageMs, tiles },
    setupHint: Boolean(fields && missingBlock && (legacy || (current && otherFresh))),
  };
}
export type LoadoutViewModel = ReturnType<typeof deriveLoadoutViewModel>;

export function abilityStatusText(tile: AbilityTile, locale: "th" | "en"): string {
  return tile.status === "cooldown" && tile.cooldown !== null ? `${formatCooldownValue(tile.cooldown)}s`
    : tile.status === "ready" ? locale === "th" ? "พร้อม" : "Ready" : "—";
}
