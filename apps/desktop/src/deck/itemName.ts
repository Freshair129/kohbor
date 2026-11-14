export interface ItemName { name: string; rawId: string }
// General-knowledge aliases supplied for display only. UNVERIFIED against the current patch.
const EXCEPTIONS: Readonly<Record<string, string>> = {
  blink: "Blink Dagger", black_king_bar: "Black King Bar", tpscroll: "Town Portal Scroll", travel_boots: "Boots of Travel",
  power_treads: "Power Treads", magic_wand: "Magic Wand", ultimate_scepter: "Aghanim's Scepter", ultimate_scepter_2: "Aghanim's Scepter",
  aghanims_shard: "Aghanim's Shard", cyclone: "Eul's Scepter of Divinity", sheepstick: "Scythe of Vyse", heart: "Heart of Tarrasque",
  greater_crit: "Daedalus", lifesteal: "Morbid Mask", force_staff: "Force Staff", glimmer_cape: "Glimmer Cape", mjollnir: "Mjollnir", bfury: "Battle Fury",
  boots: "Boots of Speed", arcane_boots: "Arcane Boots", phase_boots: "Phase Boots", refresher: "Refresher Orb",
  ward_observer: "Observer Ward", ward_sentry: "Sentry Ward", dust: "Dust of Appearance", gem: "Gem of True Sight", smoke_of_deceit: "Smoke of Deceit",
};
export function displayItemName(itemId: string | null): ItemName | null {
  if (!itemId?.trim()) return null;
  if (!itemId.startsWith("item_")) return { name: itemId, rawId: itemId };
  const internal = itemId.slice(5);
  const name = internal.startsWith("recipe_") ? "Recipe" : Object.hasOwn(EXCEPTIONS, internal) ? EXCEPTIONS[internal]!
    : internal.split("_").filter(Boolean).map(word => word.charAt(0).toUpperCase() + word.slice(1).toLowerCase()).join(" ");
  return { name: name || itemId, rawId: itemId };
}
