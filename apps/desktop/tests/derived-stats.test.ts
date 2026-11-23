import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";
import {
  derivedStats,
  heroAttributes,
  RULES_BY_PATCH,
  type StatsData,
} from "../src/stats/derivedStats.ts";

const stats = JSON.parse(
  readFileSync(new URL("../src/assets/dota/6989/stats.json", import.meta.url), "utf8"),
) as StatsData;

const closeTo = (actual: number | null, expected: number, tolerance = 0.000001) => {
  assert.notEqual(actual, null);
  assert.ok(
    Math.abs((actual as number) - expected) <= tolerance,
    `actual ${actual} differs from expected ${expected} by more than ${tolerance}`,
  );
};

test("derives attributes for several installed heroes at levels 1, 15, and 25", () => {
  for (const heroId of ["antimage", "ember_spirit", "largo", "slark", "void_spirit"]) {
    const hero = stats.heroes[heroId];
    assert.ok(hero, "installed hero data exists for " + heroId);
    for (const level of [1, 15, 25]) {
      const derived = heroAttributes(heroId, level, [], stats);
      for (const attribute of ["strength", "agility", "intelligence"] as const) {
        const source = hero.attributes[attribute];
        closeTo(
          derived.attributes[attribute].value,
          (source.base as number) + (source.gain as number) * (level - 1),
        );
        assert.equal(derived.attributes[attribute].derived, true);
        assert.ok(derived.attributes[attribute].sources.length > 0);
      }
      assert.equal(derived.label, "DERIVED");
      assert.equal(derived.caveat, "without temporary effects");
      assert.ok(derived.limitations.includes("talents not included"));
    }
  }
});

test("adds normalized item attributes, including all attributes, and records sources", () => {
  const branch = stats.items.item_branches;
  const ogreAxe = stats.items.item_ogre_axe;
  assert.ok(branch && ogreAxe);
  const before = heroAttributes("ember_spirit", 15, [], stats).attributes;
  const after = heroAttributes("ember_spirit", 15, ["item_branches", "item_ogre_axe"], stats);

  closeTo(
    after.attributes.strength.value,
    (before.strength.value as number) +
      (branch.attributes.strength as number) +
      (branch.attributes.all as number) +
      (ogreAxe.attributes.strength as number) +
      (ogreAxe.attributes.all as number),
  );
  closeTo(
    after.attributes.agility.value,
    (before.agility.value as number) +
      (branch.attributes.agility as number) +
      (branch.attributes.all as number) +
      (ogreAxe.attributes.agility as number) +
      (ogreAxe.attributes.all as number),
  );
  assert.ok(after.attributes.strength.sources.some((source) => source.includes("item_branches")));
  assert.ok(after.attributes.strength.sources.some((source) => source.includes("item_ogre_axe")));
  assert.equal(after.attributes.strength.derived, true);

  for (const level of [1, 15, 25]) {
    const noInventory = heroAttributes("antimage", level, [], stats);
    const withBranch = heroAttributes("antimage", level, ["item_branches"], stats);
    for (const attribute of ["strength", "agility", "intelligence"] as const) {
      closeTo(
        withBranch.attributes[attribute].value,
        (noInventory.attributes[attribute].value as number) +
          (branch.attributes[attribute] as number) +
          (branch.attributes.all as number),
      );
    }
  }
});

test("selects melee or ranged boot speed from the hero attack capability", () => {
  const phaseBoots = stats.items.item_phase_boots;
  assert.ok(phaseBoots);
  assert.equal(phaseBoots.effects.moveSpeedBootsMelee, 55);
  assert.equal(phaseBoots.effects.moveSpeedBootsRanged, 45);

  const ember = derivedStats("ember_spirit", 1, ["item_phase_boots"], stats);
  assert.equal(stats.heroes.ember_spirit.attack.type, "melee");
  closeTo(
    ember.moveSpeed.value,
    stats.heroes.ember_spirit.moveSpeed! + phaseBoots.effects.moveSpeedBootsMelee!,
  );
  assert.ok(ember.moveSpeed.sources.some((source) => source.includes("moveSpeedBootsMelee")));

  const drow = derivedStats("drow_ranger", 1, ["item_phase_boots"], stats);
  assert.equal(stats.heroes.drow_ranger.attack.type, "ranged");
  closeTo(
    drow.moveSpeed.value,
    stats.heroes.drow_ranger.moveSpeed! + phaseBoots.effects.moveSpeedBootsRanged!,
  );
  assert.ok(drow.moveSpeed.sources.some((source) => source.includes("moveSpeedBootsRanged")));

  const unknownTypeStats: StatsData = {
    ...stats,
    heroes: {
      ...stats.heroes,
      ember_spirit: {
        ...stats.heroes.ember_spirit,
        attack: { ...stats.heroes.ember_spirit.attack, type: null },
      },
    },
  };
  assert.equal(
    derivedStats("ember_spirit", 1, ["item_phase_boots"], unknownTypeStats).moveSpeed.value,
    null,
  );
});

test("normalizes regen aliases and preserves unknown regen modifiers as null", () => {
  assert.equal(stats.items.item_helm_of_iron_will.effects.healthRegen, 4);
  assert.equal(stats.items.item_arcane_boots.effects.manaRegen, 1.25);

  const rules = { rules: RULES_BY_PATCH["7.42"] };
  assert.equal(
    derivedStats("ember_spirit", 15, ["item_sange"], stats, rules).healthRegen.value,
    null,
  );
  assert.equal(
    derivedStats("largo", 15, ["item_meteor_hammer"], stats, rules).manaRegen.value,
    null,
  );
  assert.equal(
    derivedStats("largo", 15, ["item_guardian_greaves"], stats, rules).healthRegen.value,
    null,
  );
});

test("matches rounded entity attributes from the real 7.41f replay", () => {
  // Source: replay-engine/tests/.cache/9093797968/match.json, entity
  // npc_dota_hero_ember_spirit, sampleT=-90, st.strT/agiT/intT=[25,25,24].
  // Tick 0 is derived as round((sampleT - t0) * dtick); tolerance is ±0.5
  // because the entity fields are integer-rounded.
  const emberLevelOne = heroAttributes(
    "ember_spirit",
    1,
    ["item_branches", "item_branches", "item_branches", "item_branches"],
    stats,
  );
  closeTo(emberLevelOne.attributes.strength.value, 25, 0.5);
  closeTo(emberLevelOne.attributes.agility.value, 25, 0.5);
  closeTo(emberLevelOne.attributes.intelligence.value, 24, 0.5);

  // Source: same match, Ember Spirit, sampleT=1398.967, lvl=15,
  // derived tick 44669, st.strT/agiT/intT=[81,81,66]. Inventory slots 0-5:
  // bottle, spirit vessel, phase boots, ogre axe, magic wand, famango.
  const emberLevelFifteen = heroAttributes(
    "ember_spirit",
    15,
    [
      "item_bottle",
      "item_spirit_vessel",
      "item_phase_boots",
      "item_ogre_axe",
      "item_magic_wand",
      "item_famango",
    ],
    stats,
    // One +2-per-attribute bonus level is an explicit, test-only input.
    // It is inferred from the entity residual after applying the recorded
    // active inventory; production derivation does not infer this value.
    { attributeBonusLevels: { levels: 1, perAttributeLevel: 2 } },
  );
  closeTo(emberLevelFifteen.attributes.strength.value, 81, 0.5);
  closeTo(emberLevelFifteen.attributes.agility.value, 81, 0.5);
  closeTo(emberLevelFifteen.attributes.intelligence.value, 66, 0.5);

  // Source: same match, Largo, sampleT=1174.967, lvl=15, derived tick
  // 37949, st.strT/agiT/intT=[96,45,88]. Inventory slots 0-5:
  // phase boots, meteor hammer, magic wand, soul ring, bracer, tiara of Selemene.
  const currentMeteorHammer = stats.items.item_meteor_hammer;
  assert.ok(currentMeteorHammer);
  // The installed 6989 script has +22 Intelligence, while the 7.41f reference
  // pack's items.json has +24. Use the observed replay-patch value only here.
  const replayMeteorHammer = {
    ...currentMeteorHammer,
    id: "item_meteor_hammer@7.41f",
    attributes: { ...currentMeteorHammer.attributes, intelligence: 24 },
  };
  const largoLevelFifteen = heroAttributes(
    "largo",
    15,
    [
      "item_phase_boots",
      replayMeteorHammer,
      "item_magic_wand",
      "item_soul_ring",
      "item_bracer",
      "item_tiara_of_selemene",
    ],
    stats,
    // Same explicit test-only +2-per-attribute level as inferred from this
    // entity reference; no production default is applied.
    { attributeBonusLevels: { levels: 1, perAttributeLevel: 2 } },
  );
  closeTo(largoLevelFifteen.attributes.strength.value, 96, 0.5);
  closeTo(largoLevelFifteen.attributes.agility.value, 45, 0.5);
  closeTo(largoLevelFifteen.attributes.intelligence.value, 88, 0.5);
});

test("derived formulas swap explicit patch rules and label every value", () => {
  const noItems: string[] = [];
  const sevenFortyOne = derivedStats("ember_spirit", 15, noItems, stats, {
    rules: RULES_BY_PATCH["7.41f"],
  });
  const sevenFortyTwo = derivedStats("ember_spirit", 15, noItems, stats, {
    rules: RULES_BY_PATCH["7.42"],
  });

  closeTo(
    sevenFortyOne.maxHealth.value,
    sevenFortyOne.attributes.strength.value! * 22 + stats.heroes.ember_spirit.health.base!,
  );
  closeTo(
    sevenFortyTwo.maxHealth.value,
    sevenFortyTwo.attributes.strength.value! * 20 + stats.heroes.ember_spirit.health.base!,
  );
  closeTo(
    sevenFortyTwo.magicalResistance.value,
    (stats.heroes.ember_spirit.magicResistance! +
      sevenFortyTwo.attributes.intelligence.value! * 0.125) /
      100,
  );
  assert.notEqual(sevenFortyOne.maxHealth.value, sevenFortyTwo.maxHealth.value);
  for (const key of [
    "armor",
    "physicalDamageReduction",
    "magicalResistance",
    "attackSpeed",
    "attacksPerSecond",
    "damageMin",
    "damageMax",
    "moveSpeed",
    "maxHealth",
    "healthRegen",
    "evasion",
    "statusResistance",
  ] as const) {
    assert.equal(sevenFortyOne[key].derived, true);
    assert.ok(sevenFortyOne[key].sources.length > 0);
  }
  assert.equal(sevenFortyOne.rulesPatch, "7.41f");
  assert.equal(sevenFortyOne.bkbRules?.durationSeconds, 9);
  assert.ok(sevenFortyOne.limitations.includes("talents not included"));
});

test("applies named item formula inputs and retains their source references", () => {
  const item = {
    id: "fixture_complete_item",
    attributes: { strength: 1, agility: 1, intelligence: 1, all: 1 },
    effects: {
      armor: 2,
      attackSpeed: 15,
      damage: 5,
      moveSpeedFlat: 10,
      moveSpeedPercent: 10,
      moveSpeedBoots: 0,
      magicResistance: 20,
      evasion: 15,
      statusResistance: 20,
      spellAmp: 5,
      lifesteal: 10,
      spellLifesteal: 4,
      healthRegen: 1,
      manaRegen: 2,
    },
  };
  const derived = derivedStats("ember_spirit", 1, [item], stats, {
    rules: RULES_BY_PATCH["7.41f"],
  });
  const hero = stats.heroes.ember_spirit;
  closeTo(
    derived.armor.value,
    hero.armor! +
      derived.attributes.agility.value! * RULES_BY_PATCH["7.41f"].armorPerAgility +
      item.effects.armor,
  );
  closeTo(derived.attackSpeed.value, hero.attack.baseAttackSpeed! + derived.attributes.agility.value! + 15);
  closeTo(derived.magicalResistance.value, 1 - (1 - (hero.magicResistance! + derived.attributes.intelligence.value! * 0.1) / 100) * 0.8);
  closeTo(derived.evasion.value, 15);
  closeTo(derived.statusResistance.value, 20);
  closeTo(derived.spellAmp.value, 5);
  closeTo(derived.lifesteal.value, 10);
  closeTo(derived.spellLifesteal.value, 4);
  closeTo(derived.moveSpeed.value, Math.min(550, (hero.moveSpeed! + 10) * 1.1));
  const primaryAttribute = derived.primaryAttribute.value;
  assert.ok(primaryAttribute);
  closeTo(
    derived.damageMin.value,
    hero.attack.damageMin! +
      derived.attributes[primaryAttribute].value! +
      item.effects.damage,
  );
  closeTo(
    derived.healthRegen.value,
    hero.health.regen! +
      derived.attributes.strength.value! * RULES_BY_PATCH["7.41f"].hpRegenPerStrength +
      item.effects.healthRegen,
  );
  closeTo(derived.manaRegen.value, hero.mana.regen! + item.effects.manaRegen);
  closeTo(
    derived.attacksPerSecond.value,
    derived.attackSpeed.value! / 100 / hero.attack.baseAttackTime!,
  );
  assert.ok(derived.armor.sources.some((source) => source.includes("fixture_complete_item")));
  assert.equal(derived.spellAmp.derived, true);
});

test("unknown inputs stay null and explicit bonuses are included only when passed", () => {
  assert.equal(heroAttributes("not_a_hero", 1, [], stats).attributes.strength.value, null);
  assert.equal(heroAttributes("ember_spirit", 0, [], stats).attributes.strength.value, null);
  assert.equal(heroAttributes("ember_spirit", 1, null, stats).attributes.strength.value, null);
  assert.equal(
    derivedStats("ember_spirit", 1, ["item_does_not_exist"], stats, {
      rules: RULES_BY_PATCH["7.41f"],
    }).attributes.strength.value,
    null,
  );
  assert.equal(
    heroAttributes("slark", 15, ["item_power_treads"], stats).attributes.strength.value,
    null,
  );
  const withoutRules = derivedStats("ember_spirit", 1, [], stats);
  assert.equal(withoutRules.armor.value, null);
  assert.equal(withoutRules.maxHealth.value, null);
  assert.equal(withoutRules.magicalResistance.value, null);

  const universal = derivedStats("void_spirit", 25, [], stats, {
    rules: RULES_BY_PATCH["7.42"],
  });
  assert.equal(universal.primaryAttribute.value, "universal");
  assert.equal(universal.damageMin.value, null);
  assert.ok(universal.limitations.includes("universal primary damage formula not included"));

  const ordinary = heroAttributes("ember_spirit", 15, [], stats);
  const explicit = heroAttributes("ember_spirit", 15, [], stats, {
    talentBonuses: { strength: 3, agility: 0, intelligence: 0 },
    attributeBonusLevels: { levels: 2, perAttributeLevel: 2 },
  });
  closeTo(
    explicit.attributes.strength.value,
    (ordinary.attributes.strength.value as number) + 3 + 4,
  );
  assert.equal(explicit.talentsIncluded, true);
  assert.equal(explicit.attributeBonusLevelsIncluded, true);
  assert.ok(explicit.attributes.strength.sources.includes("talent:strength:explicit"));
});
