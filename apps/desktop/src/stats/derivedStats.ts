/**
 * Pure own-hero stat derivation from installed-game data and an explicit item
 * inventory. Results are estimates without temporary effects.
 */

export type DerivedValue<T = number> = {
  value: T | null;
  sources: string[];
  derived: true;
};

export type AttributeName = "strength" | "agility" | "intelligence";

export type HeroGameStats = {
  primaryAttribute: AttributeName | "universal" | null;
  attributes: Record<AttributeName, { base: number | null; gain: number | null }>;
  armor: number | null;
  attack: {
    type: "melee" | "ranged" | null;
    baseAttackSpeed: number | null;
    baseAttackTime: number | null;
    damageMin: number | null;
    damageMax: number | null;
    range: number | null;
  };
  moveSpeed: number | null;
  health: { base: number | null; regen: number | null };
  mana: { base: number | null; regen: number | null };
  magicResistance: number | null;
  sources?: string[];
};

export type ItemGameStats = {
  attributes: {
    strength: number | null;
    agility: number | null;
    intelligence: number | null;
    all: number | null;
  };
  effects: {
    armor: number | null;
    attackSpeed: number | null;
    damage: number | null;
    moveSpeedFlat: number | null;
    moveSpeedPercent: number | null;
    moveSpeedBoots?: number | null;
    moveSpeedBootsMelee?: number | null;
    moveSpeedBootsRanged?: number | null;
    magicResistance: number | null;
    evasion: number | null;
    statusResistance: number | null;
    spellAmp: number | null;
    lifesteal: number | null;
    spellLifesteal: number | null;
    healthRegen: number | null;
    manaRegen: number | null;
  };
  unknown?: Record<string, string>;
};

export type StatsData = {
  build?: number;
  patch?: string;
  heroes: Record<string, HeroGameStats>;
  items: Record<string, ItemGameStats>;
};

export type DerivedRules = {
  patch: "7.41f" | "7.42";
  hpPerStrength: number;
  hpRegenPerStrength: number;
  mrPercentPerIntelligence: number;
  armorPerAgility: number;
  bkb: {
    durationSeconds: number;
    magicResistancePercent: number;
    magicImmunityPierces?: boolean;
  };
};

export type ExplicitBonuses = {
  talentBonuses?: Record<AttributeName, number>;
  /** Caller-resolved per-attribute value; no implicit talent selection is made. */
  attributeBonusLevels?: { levels: number; perAttributeLevel: number };
};

export const RULES_BY_PATCH: Readonly<Record<DerivedRules["patch"], DerivedRules>> = {
  "7.41f": {
    patch: "7.41f",
    hpPerStrength: 22,
    hpRegenPerStrength: 0.1,
    mrPercentPerIntelligence: 0.1,
    armorPerAgility: 0.166667,
    bkb: {
      durationSeconds: 9,
      magicResistancePercent: 60,
      magicImmunityPierces: false,
    },
  },
  "7.42": {
    patch: "7.42",
    hpPerStrength: 20,
    hpRegenPerStrength: 0.1,
    mrPercentPerIntelligence: 0.125,
    armorPerAgility: 0.166667,
    bkb: {
      durationSeconds: 9,
      magicResistancePercent: 60,
      magicImmunityPierces: true,
    },
  },
};

const ATTRIBUTE_NAMES: readonly AttributeName[] = [
  "strength",
  "agility",
  "intelligence",
];

function nullValue(sources: string[] = []): DerivedValue {
  return { value: null, sources: unique(sources), derived: true };
}

function result(value: number | null, sources: string[]): DerivedValue {
  return {
    value: value !== null && Number.isFinite(value) ? value : null,
    sources: unique(sources),
    derived: true,
  };
}

function unique(values: string[]): string[] {
  return [...new Set(values)];
}

function isFiniteNumber(input: unknown): input is number {
  return typeof input === "number" && Number.isFinite(input);
}

function heroRef(heroId: string, field: string): string {
  return "game:heroes." + heroId + "." + field;
}

type ResolvedItem = { id: string; item: ItemGameStats } | null;
export type ItemInput = string | (ItemGameStats & { id?: string }) | null | undefined;

function resolveItems(
  items: readonly ItemInput[] | null | undefined,
  stats: StatsData | null | undefined,
): ResolvedItem[] {
  if (!Array.isArray(items)) return [null];
  return items.map((entry) => {
    if (typeof entry === "string") {
      const item = stats?.items?.[entry];
      return item ? { id: entry, item } : null;
    }
    if (!entry || typeof entry !== "object") return null;
    return { id: entry.id ?? "inline-item", item: entry };
  });
}

function sumItemEffect(
  resolved: ResolvedItem[],
  field: keyof ItemGameStats["effects"],
): { value: number | null; sources: string[] } {
  let total = 0;
  const sources: string[] = [];
  if (resolved.length === 0) sources.push("inventory:empty");
  for (const entry of resolved) {
    if (!entry) return { value: null, sources: [...sources, "inventory:unknown-item"] };
    const amount = entry.item.effects?.[field];
    if (!isFiniteNumber(amount)) {
      return {
        value: null,
        sources: [...sources, "game:items." + entry.id + ".effects." + field],
      };
    }
    total += amount;
    sources.push("game:items." + entry.id + ".effects." + field);
  }
  return { value: total, sources };
}

function sumMoveSpeed(
  resolved: ResolvedItem[],
  heroId: string,
  attackType: "melee" | "ranged" | null,
): { value: number | null; sources: string[] } {
  let flat = 0;
  let bestBoots = 0;
  const sources: string[] = [];
  if (resolved.length === 0) sources.push("inventory:empty");
  for (const entry of resolved) {
    if (!entry) return { value: null, sources: [...sources, "inventory:unknown-item"] };
    const effects = entry.item.effects;
    if (!isFiniteNumber(effects?.moveSpeedFlat)) {
      return {
        value: null,
        sources: [...sources, "game:items." + entry.id + ".effects.moveSpeedFlat"],
      };
    }
    flat += effects.moveSpeedFlat;
    sources.push("game:items." + entry.id + ".effects.moveSpeedFlat");

    const boots = effects.moveSpeedBoots;
    if (boots === null) {
      return {
        value: null,
        sources: [...sources, "game:items." + entry.id + ".effects.moveSpeedBoots"],
      };
    }
    if (isFiniteNumber(boots) && boots > 0) {
      bestBoots = Math.max(bestBoots, boots);
      sources.push("game:items." + entry.id + ".effects.moveSpeedBoots");
    }

    const meleeBoots = effects.moveSpeedBootsMelee;
    const rangedBoots = effects.moveSpeedBootsRanged;
    let selectedBoots: number | null = null;
    let selectedField: string | null = null;
    if (attackType === "melee") {
      if (isFiniteNumber(meleeBoots)) {
        selectedBoots = meleeBoots;
        selectedField = "moveSpeedBootsMelee";
      }
    } else if (attackType === "ranged") {
      if (isFiniteNumber(rangedBoots)) {
        selectedBoots = rangedBoots;
        selectedField = "moveSpeedBootsRanged";
      }
    } else if (
      isFiniteNumber(meleeBoots) &&
      isFiniteNumber(rangedBoots) &&
      meleeBoots === rangedBoots
    ) {
      selectedBoots = meleeBoots;
      selectedField = "moveSpeedBootsMelee/moveSpeedBootsRanged";
    } else if (
      (isFiniteNumber(meleeBoots) && meleeBoots !== 0) ||
      (isFiniteNumber(rangedBoots) && rangedBoots !== 0) ||
      meleeBoots === null ||
      rangedBoots === null
    ) {
      return {
        value: null,
        sources: [
          ...sources,
          heroRef(heroId, "attack.type"),
          "game:items." + entry.id + ".effects.moveSpeedBootsMelee",
          "game:items." + entry.id + ".effects.moveSpeedBootsRanged",
        ],
      };
    }
    if (selectedBoots !== null && selectedField !== null) {
      bestBoots = Math.max(bestBoots, selectedBoots);
      if (selectedBoots !== 0) {
        sources.push("game:items." + entry.id + ".effects." + selectedField);
      }
    } else if (
      (attackType === "melee" && meleeBoots === null) ||
      (attackType === "ranged" && rangedBoots === null)
    ) {
      return {
        value: null,
        sources: [
          ...sources,
          "game:items." + entry.id + ".effects." +
            (attackType === "melee" ? "moveSpeedBootsMelee" : "moveSpeedBootsRanged"),
        ],
      };
    }
  }
  return { value: flat + bestBoots, sources };
}

function sumAttributes(
  hero: HeroGameStats | undefined,
  attribute: AttributeName,
  level: number,
  resolved: ResolvedItem[],
  bonuses: ExplicitBonuses,
  heroId: string,
): DerivedValue {
  const sources: string[] = [];
  const base = hero?.attributes?.[attribute]?.base;
  const gain = hero?.attributes?.[attribute]?.gain;
  const fields = [
    heroRef(heroId, "attributes." + attribute + ".base"),
    heroRef(heroId, "attributes." + attribute + ".gain"),
    "level:" + level,
  ];
  if (!isFiniteNumber(base) || !isFiniteNumber(gain) || !Number.isInteger(level) || level < 1) {
    return nullValue(fields);
  }

  let total = base + gain * (level - 1);
  sources.push(...fields);
  for (const entry of resolved) {
    if (!entry) return nullValue([...sources, "inventory:unknown-item"]);
    const itemAttributes = entry.item.attributes;
    const direct = itemAttributes?.[attribute];
    const all = itemAttributes?.all;
    if (!isFiniteNumber(direct) || !isFiniteNumber(all)) {
      return nullValue([
        ...sources,
        "game:items." + entry.id + ".attributes." + attribute,
        "game:items." + entry.id + ".attributes.all",
      ]);
    }
    total += direct + all;
    sources.push(
      "game:items." + entry.id + ".attributes." + attribute,
      "game:items." + entry.id + ".attributes.all",
    );
  }
  if (resolved.length === 0) sources.push("inventory:empty");

  const talent = bonuses.talentBonuses?.[attribute];
  if (talent !== undefined) {
    if (!isFiniteNumber(talent)) return nullValue([...sources, "talent:" + attribute]);
    total += talent;
    sources.push("talent:" + attribute + ":explicit");
  } else {
    sources.push("talents:not-included");
  }
  const attributeBonus = bonuses.attributeBonusLevels;
  if (attributeBonus) {
    if (
      !isFiniteNumber(attributeBonus.levels) ||
      !isFiniteNumber(attributeBonus.perAttributeLevel)
    ) {
      return nullValue([...sources, "attribute-bonus-levels:unknown"]);
    }
    total += attributeBonus.levels * attributeBonus.perAttributeLevel;
    sources.push(
      "attribute-bonus-levels:" +
        attributeBonus.levels +
        "x" +
        attributeBonus.perAttributeLevel,
    );
  } else {
    sources.push("attribute-bonus-levels:not-included");
  }
  return result(total, sources);
}

export function heroAttributes(
  heroId: string,
  level: number,
  items: readonly ItemInput[] | null | undefined,
  stats: StatsData | null | undefined,
  bonuses?: ExplicitBonuses,
) {
  const hero = stats?.heroes?.[heroId];
  const resolved = resolveItems(items, stats);
  const explicit = bonuses ?? {};
  const attributes = Object.fromEntries(
    ATTRIBUTE_NAMES.map((attribute) => [
      attribute,
      sumAttributes(hero, attribute, level, resolved, explicit, heroId),
    ]),
  ) as Record<AttributeName, DerivedValue>;
  const talentIncluded = Boolean(explicit.talentBonuses);
  const attributeBonusIncluded = Boolean(explicit.attributeBonusLevels);
  return {
    heroId,
    level,
    label: "DERIVED",
    caveat: "without temporary effects",
    talentsIncluded: talentIncluded,
    attributeBonusLevelsIncluded: attributeBonusIncluded,
    limitations: [
      ...(talentIncluded ? [] : ["talents not included"]),
      ...(attributeBonusIncluded ? [] : ["attribute-bonus levels not included"]),
      ...(hero?.primaryAttribute === "universal"
        ? ["universal primary damage formula not included"]
        : []),
    ],
    primaryAttribute: {
      value: hero?.primaryAttribute ?? null,
      sources: hero ? [heroRef(heroId, "primaryAttribute")] : [],
      derived: true as const,
    },
    attributes,
  };
}

function sumOrNull(...values: Array<number | null>): number | null {
  if (values.some((entry) => !isFiniteNumber(entry))) return null;
  return values.reduce<number>((sum, entry) => sum + (entry as number), 0);
}

function percentageStack(values: readonly number[]): number {
  return 1 - values.reduce((remaining, percent) => remaining * (1 - percent / 100), 1);
}

function itemValues(
  resolved: ResolvedItem[],
  field: keyof ItemGameStats["effects"],
): { value: number | null; sources: string[]; values: number[] } {
  const summed = sumItemEffect(resolved, field);
  const values = resolved.map((entry) => {
    if (!entry) return null;
    const amount = entry.item.effects?.[field];
    return isFiniteNumber(amount) ? amount : null;
  });
  return { ...summed, values: values.filter(isFiniteNumber) };
}

export function derivedStats(
  heroId: string,
  level: number,
  items: readonly ItemInput[] | null | undefined,
  stats: StatsData | null | undefined,
  options: ExplicitBonuses & { rules?: DerivedRules } = {},
) {
  const hero = stats?.heroes?.[heroId];
  const resolved = resolveItems(items, stats);
  const attributes = heroAttributes(heroId, level, items, stats, options);
  const strength = attributes.attributes.strength;
  const agility = attributes.attributes.agility;
  const intelligence = attributes.attributes.intelligence;
  const rules = options.rules;
  const armorItems = itemValues(resolved, "armor");
  const speedItems = itemValues(resolved, "attackSpeed");
  const damageItems = itemValues(resolved, "damage");
  const moveItems = sumMoveSpeed(resolved, heroId, hero?.attack?.type ?? null);
  const movePercent = itemValues(resolved, "moveSpeedPercent");
  const mrItems = itemValues(resolved, "magicResistance");
  const evasionItems = itemValues(resolved, "evasion");
  const statusItems = itemValues(resolved, "statusResistance");
  const ampItems = itemValues(resolved, "spellAmp");
  const lifestealItems = itemValues(resolved, "lifesteal");
  const spellLifestealItems = itemValues(resolved, "spellLifesteal");
  const healthRegenItems = itemValues(resolved, "healthRegen");
  const manaRegenItems = itemValues(resolved, "manaRegen");

  // Non-rule formulas port derived() from replay-viewer-proto/viewer.js and
  // apps/desktop/src/replay/matchViewer.mjs; the prototype cites Liquipedia
  // for the attack-speed and physical-reduction rules noted below.
  // Rule constants are from replay-engine-rs pack/<patch>/pack.json at a609ecc.
  const armorValue =
    rules && isFiniteNumber(hero?.armor) && isFiniteNumber(agility.value) && armorItems.value !== null
      ? hero.armor + agility.value * rules.armorPerAgility + armorItems.value
      : null;
  // Ported from prototype derived(): 0.06*A/(1+0.06*|A|), per its Liquipedia note.
  const physicalReduction =
    armorValue === null ? null : (0.06 * armorValue) / (1 + 0.06 * Math.abs(armorValue));
  // Base hero MR and INT contribution add; item MR sources stack multiplicatively.
  const mrBase =
    rules && isFiniteNumber(hero?.magicResistance) && isFiniteNumber(intelligence.value)
      ? (hero.magicResistance + intelligence.value * rules.mrPercentPerIntelligence) / 100
      : null;
  const magicalResistance =
    mrBase === null || mrItems.values.length !== resolved.length || mrItems.value === null
      ? null
      : 1 - (1 - mrBase) * (1 - percentageStack(mrItems.values));

  const attackSpeedValue =
    isFiniteNumber(hero?.attack?.baseAttackSpeed) &&
    isFiniteNumber(agility.value) &&
    speedItems.value !== null
      ? Math.max(
          20 + hero.attack.baseAttackSpeed - 100,
          Math.min(700, hero.attack.baseAttackSpeed + agility.value + speedItems.value),
        )
      : null;
  // Attack-speed limits and APS formula are ported from the prototype's
  // derived() comment, which cites Liquipedia's attack-speed rules.
  const attacksPerSecond =
    isFiniteNumber(attackSpeedValue) &&
    isFiniteNumber(hero?.attack?.baseAttackTime) &&
    hero.attack.baseAttackTime > 0
      ? attackSpeedValue / 100 / hero.attack.baseAttackTime
      : null;

  const moveSpeedBase =
    isFiniteNumber(hero?.moveSpeed) && moveItems.value !== null && movePercent.value !== null
      ? (hero.moveSpeed + moveItems.value) * (1 + movePercent.value / 100)
      : null;
  // The prototype clamps move speed to 100–550; temporary haste is not inferred.
  const moveSpeed =
    moveSpeedBase === null ? null : Math.max(100, Math.min(550, moveSpeedBase));

  // Ported from the prototype: base damage + primary attribute + item damage.
  // Base damage comes from the installed hero script and item values are
  // normalized from AbilityValues/attrib.
  const primaryAttribute = hero?.primaryAttribute;
  const primaryValue =
    primaryAttribute && primaryAttribute !== "universal"
      ? attributes.attributes[primaryAttribute]?.value
      : null;
  const primaryAttributeSources =
    primaryAttribute === "universal"
      ? ["limit:universal-primary-damage-formula-not-included"]
      : primaryAttribute
        ? attributes.attributes[primaryAttribute].sources
        : [];
  const damageMin = sumOrNull(
    hero?.attack?.damageMin ?? null,
    primaryValue ?? null,
    damageItems.value,
  );
  const damageMax = sumOrNull(
    hero?.attack?.damageMax ?? null,
    primaryValue ?? null,
    damageItems.value,
  );

  const maxHealth =
    rules && isFiniteNumber(hero?.health?.base) && isFiniteNumber(strength.value)
      ? hero.health.base + strength.value * rules.hpPerStrength
      : null;
  const healthRegen =
    rules &&
    isFiniteNumber(hero?.health?.regen) &&
    isFiniteNumber(strength.value) &&
    healthRegenItems.value !== null
      ? hero.health.regen + strength.value * rules.hpRegenPerStrength + healthRegenItems.value
      : null;
  // No mana-per-INT rule is in the replay pack; this is script base plus items.
  const manaRegen =
    isFiniteNumber(hero?.mana?.regen) && manaRegenItems.value !== null
      ? hero.mana.regen + manaRegenItems.value
      : null;
  const armorSources = [
    heroRef(heroId, "armor"),
    ...attributes.attributes.agility.sources,
    "rules:" + (rules?.patch ?? "unknown") + ".armorPerAgility",
    ...armorItems.sources,
  ];
  const mrSources = [
    heroRef(heroId, "magicResistance"),
    ...attributes.attributes.intelligence.sources,
    "rules:" + (rules?.patch ?? "unknown") + ".mrPercentPerIntelligence",
    ...mrItems.sources,
  ];
  const sourcesOf = (entry: { sources: string[] }) => entry.sources;
  return {
    heroId,
    level,
    label: "DERIVED",
    caveat: "without temporary effects",
    talentsIncluded: attributes.talentsIncluded,
    attributeBonusLevelsIncluded: attributes.attributeBonusLevelsIncluded,
    limitations: attributes.limitations,
    rulesPatch: rules?.patch ?? null,
    bkbRules: rules?.bkb ?? null,
    attributes: attributes.attributes,
    primaryAttribute: attributes.primaryAttribute,
    armor: result(armorValue, armorSources),
    physicalDamageReduction: result(physicalReduction, [
      "formula:0.06*armor/(1+0.06*abs(armor))",
      ...armorSources,
    ]),
    magicalResistance: result(magicalResistance, mrSources),
    attackSpeed: result(attackSpeedValue, [
      heroRef(heroId, "attack.baseAttackSpeed"),
      ...attributes.attributes.agility.sources,
      ...speedItems.sources,
      "formula:attack-speed-clamp-20-to-700",
    ]),
    attacksPerSecond: result(attacksPerSecond, [
      "formula:attackSpeed/100/baseAttackTime",
      heroRef(heroId, "attack.baseAttackTime"),
      heroRef(heroId, "attack.baseAttackSpeed"),
      ...attributes.attributes.agility.sources,
      ...speedItems.sources,
    ]),
    damageMin: result(damageMin, [
      heroRef(heroId, "attack.damageMin"),
      "primary-attribute:" + (primaryAttribute ?? "unknown"),
      ...primaryAttributeSources,
      ...damageItems.sources,
    ]),
    damageMax: result(damageMax, [
      heroRef(heroId, "attack.damageMax"),
      "primary-attribute:" + (primaryAttribute ?? "unknown"),
      ...primaryAttributeSources,
      ...damageItems.sources,
    ]),
    attackRange: result(hero?.attack?.range ?? null, [heroRef(heroId, "attack.range")]),
    moveSpeed: result(moveSpeed, [
      heroRef(heroId, "moveSpeed"),
      heroRef(heroId, "attack.type"),
      ...moveItems.sources,
      ...movePercent.sources,
      "formula:movement-speed-clamp-100-to-550",
    ]),
    maxHealth: result(maxHealth, [
      heroRef(heroId, "health.base"),
      ...attributes.attributes.strength.sources,
      "rules:" + (rules?.patch ?? "unknown") + ".hpPerStrength",
      "limit:item-max-health-bonus-not-included",
    ]),
    healthRegen: result(healthRegen, [
      heroRef(heroId, "health.regen"),
      ...attributes.attributes.strength.sources,
      "rules:" + (rules?.patch ?? "unknown") + ".hpRegenPerStrength",
      ...healthRegenItems.sources,
    ]),
    baseMana: result(hero?.mana?.base ?? null, [heroRef(heroId, "mana.base")]),
    manaRegen: result(manaRegen, [
      heroRef(heroId, "mana.regen"),
      ...manaRegenItems.sources,
      "limit:no-mana-per-intelligence-rule",
    ]),
    evasion: result(
      hero && evasionItems.values.length === resolved.length
        ? percentageStack(evasionItems.values) * 100
        : null,
      sourcesOf(evasionItems),
    ),
    statusResistance: result(
      hero && statusItems.values.length === resolved.length
        ? percentageStack(statusItems.values) * 100
        : null,
      sourcesOf(statusItems),
    ),
    spellAmp: result(hero ? ampItems.value : null, sourcesOf(ampItems)),
    lifesteal: result(hero ? lifestealItems.value : null, sourcesOf(lifestealItems)),
    spellLifesteal: result(
      hero ? spellLifestealItems.value : null,
      sourcesOf(spellLifestealItems),
    ),
  };
}
