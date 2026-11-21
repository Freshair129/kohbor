// @ts-ignore Explicit extensions support Node's TypeScript loader.
import type { OwnSnapshot } from "../types.ts";
// @ts-ignore Explicit extensions support Node's TypeScript loader.
import { ITEM_SLOTS } from "../ownLoadout.ts";

/** SYNTHETIC SAMPLE DATA: imported only by the development mock harness. No assets. */
export function addSampleLoadout(base: OwnSnapshot): OwnSnapshot {
  const snapshot = structuredClone(base);
  snapshot.schema_version = "2.2.0";
  if (!snapshot.own) return snapshot;
  const availability = snapshot.availability === "stale" ? "stale" : "fresh";
  const prefix = snapshot.own.fields.hero_id.value?.replace(/^npc_dota_hero_/, "") ?? "sample";
  const syntheticItems = {
    inventory0: { name: "item_blink", cooldown: 12, charges: null },
    inventory1: { name: "item_magic_wand", cooldown: null, charges: 3 },
    inventory2: { name: "item_black_king_bar", cooldown: null, charges: null },
    inventory3: { name: "item_force_staff", cooldown: null, charges: null },
    inventory4: { name: "item_power_treads", cooldown: null, charges: null },
    inventory5: { name: null, cooldown: null, charges: null },
    backpack0: { name: "item_dust", cooldown: null, charges: 2 },
    backpack1: { name: null, cooldown: null, charges: null },
    backpack2: { name: null, cooldown: null, charges: null },
    teleport: { name: "item_tpscroll", cooldown: null, charges: 3 },
    neutral: { name: "item_pupils_gift", cooldown: null, charges: null },
  } as const;
  snapshot.own.fields.items = { availability, value: ITEM_SLOTS.map(slot => ({ slot,
    ...syntheticItems[slot], can_cast: null, passive: null })) };
  snapshot.own.fields.abilities = { availability, value: [
    { slot: 0, name: `${prefix}_sample_ability_one`, level: 1, cooldown: 0, ultimate: false, can_cast: true, passive: false, active: true },
    { slot: 1, name: `${prefix}_sample_ability_two`, level: 2, cooldown: 8, ultimate: false, can_cast: false, passive: false, active: true },
    { slot: 2, name: `${prefix}_sample_ability_three`, level: 2, cooldown: 0, ultimate: false, can_cast: true, passive: false, active: true },
    { slot: 5, name: `${prefix}_sample_ultimate`, level: 1, cooldown: 0, ultimate: true, can_cast: true, passive: false, active: true },
    { slot: 6, name: "plus_high_five", level: 1, cooldown: 0, ultimate: false, can_cast: true, passive: false, active: true },
    { slot: 7, name: "seasonal_dark_carnival", level: 1, cooldown: 22, ultimate: false, can_cast: false, passive: false, active: true },
  ] };
  return snapshot;
}
