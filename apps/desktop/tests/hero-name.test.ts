import assert from "node:assert/strict";
import { test } from "node:test";
import { displayHeroName, heroNameCaption } from "../src/deck/heroName.ts";

test("absent and empty hero names remain unknown", () => {
  for (const input of [null, "", "   ", "npc_dota_hero_"]) assert.equal(displayHeroName(input), null);
});

test("Lion is a display name while its original GSI id is preserved", () => {
  assert.deepEqual(displayHeroName("npc_dota_hero_lion"), { name: "Lion", rawId: "npc_dota_hero_lion" });
});

test("internal aliases use their supplied in-game exception names", () => {
  for (const [internal, name] of [["nevermore", "Shadow Fiend"], ["zuus", "Zeus"], ["wisp", "Io"],
    ["furion", "Nature's Prophet"], ["antimage", "Anti-Mage"], ["obsidian_destroyer", "Outworld Destroyer"],
    ["keeper_of_the_light", "Keeper of the Light"], ["skeleton_king", "Wraith King"]]) {
    assert.equal(displayHeroName(`npc_dota_hero_${internal}`)?.name, name);
  }
});

test("unknown internal names fall back to title case for each underscore-delimited word", () => {
  assert.equal(displayHeroName("npc_dota_hero_muerta")?.name, "Muerta");
  assert.equal(displayHeroName("npc_dota_hero_future_hero")?.name, "Future Hero");
});

test("a malformed id without the prefix is shown exactly as given", () => {
  assert.deepEqual(displayHeroName("unrecognised_RAW_id"), { name: "unrecognised_RAW_id", rawId: "unrecognised_RAW_id" });
});

test("only fallback names that differ from the id receive a muted raw-id caption", () => {
  assert.equal(heroNameCaption("npc_dota_hero_lion"), "npc_dota_hero_lion");
  assert.equal(heroNameCaption("npc_dota_hero_nevermore"), null);
  assert.equal(heroNameCaption("npc_dota_hero_tiny"), null);
  assert.equal(heroNameCaption("raw_id"), null);
  assert.equal(heroNameCaption(null), null);
});
