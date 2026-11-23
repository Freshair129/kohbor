import pack741 from "../assets/replay-packs/7.41f/replay-data.json" with { type: "json" };
import pack742 from "../assets/replay-packs/7.42/replay-data.json" with { type: "json" };
import installedStats from "../assets/dota/6989/stats.json" with { type: "json" };

const PACKS = new Map([["7.41f", pack741], ["7.42", pack742]]);
const CONTROL_MODIFIER_TERMS = [
  "stun", "hex", "voodoo", "silence", "root", "ensnare", "bash", "sleep", "fear", "taunt",
  "cyclone", "freeze", "entangle", "shackle", "leash", "disarm", "impale", "knockback", "banish", "bind",
];
const slug = heroId => String(heroId ?? "").replace(/^npc_dota_hero_/, "");

export function installedAbilityData(stats, semanticPack = null) {
  const heroRows = stats && typeof stats.heroes === "object" && stats.heroes !== null ? stats.heroes : {};
  const semanticControls = semanticPack?.control_abilities_by_hero ?? {};
  const semanticSources = semanticPack?.control_ability_source ?? {};
  const abilityNamesByHero = {};
  const controlAbilitiesByHero = {};
  const controlAbilitySource = {};
  const abilitiesByHero = {};
  for (const [heroName, hero] of Object.entries(heroRows)) {
    const heroSlug = slug(heroName);
    const abilities = Array.isArray(hero?.abilities) ? hero.abilities : [];
    const valid = abilities.filter(ability => typeof ability?.name === "string");
    abilityNamesByHero[heroSlug] = valid.map(ability => ability.name);
    abilitiesByHero[heroSlug] = valid.filter(ability => ability.ultimate === true).map(ability => ability.name);
    const controls = valid.filter(ability => {
      const tokens = [
        ...(Array.isArray(ability.modifiers) ? ability.modifiers : []),
        ...(Array.isArray(ability.behavior) ? ability.behavior : []),
      ].map(value => String(value).toLowerCase());
      const installedControlFlag = tokens.some(token => CONTROL_MODIFIER_TERMS.some(term => token.includes(term)));
      return installedControlFlag || semanticControls[heroSlug]?.includes(ability.name) === true;
    });
    controlAbilitiesByHero[heroSlug] = controls.map(ability => ability.name);
    for (const ability of controls) {
      controlAbilitySource[ability.name] = {
        ...(semanticSources[ability.name] ?? { dname: null, desc: "" }),
        modifiers: Array.isArray(ability.modifiers) ? ability.modifiers : [],
        behavior: Array.isArray(ability.behavior) ? ability.behavior : [],
      };
    }
  }
  const available = Object.values(abilitiesByHero).some(abilities => abilities.length > 0);
  return {
    ability_names_by_hero: abilityNamesByHero,
    control_abilities_by_hero: controlAbilitiesByHero,
    control_ability_source: controlAbilitySource,
    ultimate_classification: {
      available,
      reason: available ? "Hero slots and AbilityType exported from installed game files." : "Installed stats contain no verified ultimate slots.",
      abilities_by_hero: available ? abilitiesByHero : null,
    },
    ability_data_source: {
      kind: "installed_game_build",
      build: Number.isInteger(stats?.build) ? stats.build : null,
      patch: typeof stats?.patch === "string" ? stats.patch : "unknown",
      control_semantics: "replay-pack CC markers intersected with installed hero ability slots",
    },
  };
}

function withInstalledAbilities(pack, stats) {
  return { ...pack, ...installedAbilityData(stats, pack) };
}

export function replayPackForPatch(patch) {
  const pack = typeof patch === "string" ? PACKS.get(patch) : undefined;
  return pack ? { state: "ready", pack: withInstalledAbilities(pack, installedStats) }
    : { state: "needs_pack", patch: typeof patch === "string" ? patch : null };
}

function calibration(value, size) {
  if (!value || typeof value !== "object") return null;
  const { px, py } = value;
  if (!Array.isArray(px) || !Array.isArray(py) || px.length < 2 || py.length < 2
    || !px.slice(0, 2).every(Number.isFinite) || !py.slice(0, 2).every(Number.isFinite)) return null;
  if (!Number.isFinite(size) || size <= 0) return null;
  return { size, a: px[0], b: px[1], c: py[0], d: py[1] };
}

/** Prefer the calibration recorded in this bundle; fall back only to the matching patch pack. */
export function chooseMapCalibration(manifestCalibration, manifestSize, packMap) {
  const fromManifest = calibration(manifestCalibration, manifestSize ?? packMap?.size);
  if (fromManifest) return { calibration: fromManifest, source: "manifest" };
  const fromPack = calibration(packMap?.calibration, packMap?.size);
  return fromPack ? { calibration: fromPack, source: "pack" } : { calibration: null, source: "unknown" };
}

export function isControlEffect(pack, name) {
  if (!pack || typeof name !== "string") return false;
  const normalized = name.toLowerCase();
  return pack.control_terms.some(term => normalized.includes(term))
    || Object.values(pack.control_abilities_by_hero).some(abilities => abilities.some(ability => normalized.includes(ability)));
}

export function isUltimateAbility(pack, heroId, abilityName) {
  if (!pack || pack.ultimate_classification?.available !== true || typeof abilityName !== "string") return false;
  const abilities = pack.ultimate_classification.abilities_by_hero?.[slug(heroId)];
  return Array.isArray(abilities) && abilities.includes(abilityName);
}

export function isMobilityAbility(pack, heroId, name) {
  if (!pack || typeof name !== "string") return false;
  const abilities = pack.mobility_abilities_by_hero?.[slug(heroId)] ?? [];
  return abilities.some(ability => name.includes(ability));
}

export function isMobilityItem(pack, name) {
  return Boolean(pack && typeof name === "string" && pack.mobility_items.includes(name));
}

export function isConsumableItem(pack, name) {
  return Boolean(pack && typeof name === "string" && pack.consumable_items.includes(name));
}

export function isNotableItem(pack, name) {
  return Boolean(pack && typeof name === "string" && pack.notable_items.includes(name));
}

export function baseAttackSpeed(pack, heroId) {
  if (!pack) return null;
  const table = pack.base_attack_speed;
  return table[heroId] ?? table.default ?? null;
}
