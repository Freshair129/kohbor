import assert from "node:assert/strict";
import { existsSync, readFileSync } from "node:fs";
import test from "node:test";
import { modifierDataNeedsReparse, modifierEffectsAt, visibleStatusEffects } from "../src/replay/matchViewer.mjs";
import { statusTrayLayout } from "../src/analytics/analyticsLayoutModel.mjs";
import { resolveModifierIcon, statusIconTitle } from "../src/analytics/statusTrayModel.mjs";

const localAbility = name => existsSync(new URL(`../src/assets/dota/6993/abilities/${name}.png`, import.meta.url));

test("schema 3.2 tray reads long modifiers, unit sources and the latest positive stack count", () => {
  const hero = {
    team: 2,
    mods: [[0, 8, "modifier_enemy_slow", "npc_dota_hero_juggernaut"]],
    mods_long: [
      [1, null, "modifier_legion_commander_duel_damage_boost", "npc_dota_hero_legion_commander", ["permanent", "stacking"]],
      [2, 6, "modifier_tower_aura_bonus", { unit: "dota_goodguys_tower3_mid", team: "radiant" }, ["aura"]],
      [3, 7, "modifier_hidden_cosmetic", null, ["subtle"]],
    ],
    mod_stacks: [[1, "modifier_legion_commander_duel_damage_boost", 1], [4, "modifier_legion_commander_duel_damage_boost", 5], [7, "modifier_legion_commander_duel_damage_boost", 0]],
  };
  const byId = {
    npc_dota_hero_legion_commander: { id: "npc_dota_hero_legion_commander", team: 2, name: "Legion Commander", abil: [], casts: [] },
    npc_dota_hero_juggernaut: { id: "npc_dota_hero_juggernaut", team: 3, name: "Juggernaut", abil: [], casts: [] },
  };
  const effects = modifierEffectsAt(hero, 5, { byId, iconExists: (kind, name) => kind === "ability" && localAbility(name) });
  const byName = new Map(effects.map(effect => [effect.raw, effect]));

  assert.equal(byName.has("modifier_hidden_cosmetic"), false, "subtle modifiers stay hidden");
  assert.equal(byName.get("modifier_enemy_slow")?.kind, "debuff");
  assert.equal(byName.get("modifier_legion_commander_duel_damage_boost")?.permanent, true);
  assert.equal(byName.get("modifier_legion_commander_duel_damage_boost")?.remaining, Infinity);
  assert.equal(byName.get("modifier_legion_commander_duel_damage_boost")?.stacks, 5);
  assert.equal(byName.get("modifier_tower_aura_bonus")?.aura, true);
  assert.equal(byName.get("modifier_tower_aura_bonus")?.sourceNames[0], "ป้อม · Radiant");
  assert.equal(byName.get("modifier_tower_aura_bonus")?.icon?.icon, "tower_armor_aura");
});

test("permanent buffs lead the visible tray and stack counts appear in the tooltip", () => {
  const effects = [
    { key: "timed", name: "Timed", kind: "buff", remaining: 2, sourceNames: ["Self"] },
    { key: "permanent", name: "Duel", kind: "buff", permanent: true, remaining: Infinity, stacks: 5, sourceNames: ["Legion Commander"] },
    { key: "debuff", name: "Slow", kind: "debuff", remaining: 1, sourceNames: ["Enemy"] },
  ];
  const layout = statusTrayLayout(effects, 3);
  assert.deepEqual(visibleStatusEffects(layout).map(effect => effect.key), ["permanent", "timed", "debuff"]);
  assert.match(statusIconTitle({ name: "Duel", kind: "buff", sourceNames: ["Legion Commander"], permanent: true, stacks: 5 }), /∞ · 5 สแต็ก/);
});

test("only schema 3.0 and 3.1 require a re-parse note", () => {
  assert.equal(modifierDataNeedsReparse("3.0"), true);
  assert.equal(modifierDataNeedsReparse("3.1"), true);
  assert.equal(modifierDataNeedsReparse("3.2"), false);
  const viewer = readFileSync(new URL("../src/replay/matchViewer.mjs", import.meta.url), "utf8");
  assert.match(viewer, /Re-parse this replay with the new engine/);
  assert.match(viewer, /ต้องแยกรีเพลย์อีกครั้งด้วยเอนจินรุ่นใหม่/);
});

test("new modifier names resolve only to matching local ability icons", () => {
  const cases = [
    ["modifier_tower_aura_bonus", "tower_armor_aura"],
    ["modifier_fountain_aura_buff", "fountain_heal"],
    ["modifier_legion_commander_duel_damage_boost", "legion_commander_duel"],
    ["modifier_slark_essence_shift_permanent_buff", "slark_essence_shift"],
  ];
  for (const [modifier, expected] of cases) {
    const result = resolveModifierIcon({ modifier, iconExists: (kind, name) => kind === "ability" && localAbility(name) });
    assert.equal(result.icon, expected, `${modifier} uses the existing ${expected} asset`);
  }
});
