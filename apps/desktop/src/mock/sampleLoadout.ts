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
  snapshot.own.fields.items = { availability, value: ITEM_SLOTS.map((slot, index) => ({ slot,
    name: index === 0 ? "item_blink" : index === 1 ? "item_magic_wand" : index === 9 ? "item_tpscroll" : null,
    cooldown: index === 0 ? 12 : null, charges: index === 1 ? 3 : index === 9 ? 1 : null, can_cast: null, passive: null })) };
  snapshot.own.fields.abilities = { availability, value: [
    { slot: 0, name: `${prefix}_sample_spell`, level: 2, cooldown: 0, ultimate: false, can_cast: true, passive: false, active: true },
    { slot: 5, name: `${prefix}_sample_ultimate`, level: 1, cooldown: 45, ultimate: true, can_cast: false, passive: false, active: true },
  ] };
  return snapshot;
}
