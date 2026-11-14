export interface HeroName { name: string; rawId: string }

const PREFIX = "npc_dota_hero_";
// General-knowledge GSI aliases supplied for display only. UNVERIFIED against the current patch.
const EXCEPTIONS: Readonly<Record<string, string>> = {
  nevermore: "Shadow Fiend", zuus: "Zeus", doom_bringer: "Doom", skeleton_king: "Wraith King",
  life_stealer: "Lifestealer", wisp: "Io", obsidian_destroyer: "Outworld Destroyer", magnataur: "Magnus",
  queenofpain: "Queen of Pain", shredder: "Timbersaw", abyssal_underlord: "Underlord", rattletrap: "Clockwerk",
  necrolyte: "Necrophos", furion: "Nature's Prophet", windrunner: "Windranger", treant: "Treant Protector",
  centaur: "Centaur Warrunner", antimage: "Anti-Mage", vengefulspirit: "Vengeful Spirit", sand_king: "Sand King",
  shadow_shaman: "Shadow Shaman", witch_doctor: "Witch Doctor", templar_assassin: "Templar Assassin",
  phantom_lancer: "Phantom Lancer", phantom_assassin: "Phantom Assassin", death_prophet: "Death Prophet",
  dragon_knight: "Dragon Knight", ember_spirit: "Ember Spirit", earth_spirit: "Earth Spirit",
  storm_spirit: "Storm Spirit", void_spirit: "Void Spirit", primal_beast: "Primal Beast", dark_seer: "Dark Seer",
  dark_willow: "Dark Willow", keeper_of_the_light: "Keeper of the Light", legion_commander: "Legion Commander",
  monkey_king: "Monkey King", naga_siren: "Naga Siren", night_stalker: "Night Stalker", ogre_magi: "Ogre Magi",
  pangolier: "Pangolier", skywrath_mage: "Skywrath Mage", spirit_breaker: "Spirit Breaker",
  troll_warlord: "Troll Warlord", winter_wyvern: "Winter Wyvern", bounty_hunter: "Bounty Hunter",
  crystal_maiden: "Crystal Maiden", chaos_knight: "Chaos Knight", faceless_void: "Faceless Void",
  drow_ranger: "Drow Ranger", lone_druid: "Lone Druid", mirana: "Mirana", tiny: "Tiny",
};

export function displayHeroName(heroId: string | null): HeroName | null {
  if (heroId === null || heroId.trim().length === 0) return null;
  if (!heroId.startsWith(PREFIX)) return { name: heroId, rawId: heroId };
  const internal = heroId.slice(PREFIX.length);
  const name = Object.hasOwn(EXCEPTIONS, internal) ? EXCEPTIONS[internal]!
    : internal.split("_").filter(Boolean).map(word => word.charAt(0).toUpperCase() + word.slice(1).toLowerCase()).join(" ");
  return name.trim().length > 0 ? { name, rawId: heroId } : null;
}

/** Exceptions keep the id in a tooltip; only fallback names need a visible raw-id caption. */
export function heroNameCaption(heroId: string | null): string | null {
  const hero = displayHeroName(heroId);
  if (!hero || hero.name === hero.rawId || Object.hasOwn(EXCEPTIONS, hero.rawId.slice(PREFIX.length))) return null;
  return hero.rawId;
}
