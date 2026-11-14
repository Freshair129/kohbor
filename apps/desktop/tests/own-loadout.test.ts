import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { test } from "node:test";
import { parseOwnSnapshot } from "../src/ownSnapshot.ts";
import { ITEM_SLOTS } from "../src/ownLoadout.ts";
import { displayItemName } from "../src/deck/itemName.ts";
import { displayAbilityName } from "../src/deck/abilityName.ts";
import { abilityStatusText, deriveLoadoutViewModel } from "../src/deck/loadoutModel.ts";
import { addSampleLoadout } from "../src/mock/sampleLoadout.ts";

const item = (changes = {}) => ({ slot: "inventory0", name: "item_blink", cooldown: 12, charges: null, can_cast: false, passive: false, ...changes });
const ability = (changes = {}) => ({ slot: 0, name: "lion_impale", level: 2, cooldown: 0, ultimate: false, can_cast: true, passive: false, active: true, ...changes });
function input() {
  const snapshot = JSON.parse(readFileSync(new URL("../../../fixtures/own-fresh-at-boundary.json", import.meta.url), "utf8"));
  snapshot.schema_version = "2.2.0"; snapshot.own.fields.hero_id = { value: "npc_dota_hero_lion", availability: "fresh" };
  snapshot.own.fields.items = { value: [item(), item({ slot: "inventory1", name: null, cooldown: null })], availability: "fresh" };
  snapshot.own.fields.abilities = { value: [ability()], availability: "fresh" };
  return snapshot;
}
const parsed = (snapshot = input()) => { const value = parseOwnSnapshot(snapshot); assert.ok(value); return value; };

test("2.2 parses real arrays and empty slots without mutating the payload", () => {
  // Also exercise the host-owned boundary fixture directly as it advances to 2.2.
  assert.ok(parseOwnSnapshot(JSON.parse(readFileSync(new URL("../../../fixtures/own-fresh-at-boundary.json", import.meta.url), "utf8"))));
  const snapshot = input(); const result = parsed(snapshot);
  assert.equal(result.schema_version, "2.2.0");
  assert.deepEqual(result.own?.fields.items?.value, snapshot.own.fields.items.value);
  assert.deepEqual(result.own?.fields.abilities?.value, snapshot.own.fields.abilities.value);
  assert.notEqual(result.own?.fields.items?.value?.[0], snapshot.own.fields.items.value[0]);
});

test("missing loadout fields remain unknown in every accepted version", () => {
  for (const version of ["2.0.0", "2.1.0", "2.2.0"]) {
    const snapshot = input(); snapshot.schema_version = version;
    delete snapshot.own.fields.items; delete snapshot.own.fields.abilities;
    const result = parsed(snapshot);
    for (const key of ["items", "abilities"] as const) assert.deepEqual(result.own?.fields[key], { value: null, availability: "unknown" });
  }
});

test("invalid item entries invalidate only items and cannot appear as a partial fresh inventory", () => {
  for (const bad of [item({ slot: "inventory6" }), item({ name: "item_bad<script>" }), item({ name: "x".repeat(65) }),
    item({ name: undefined }), item({ cooldown: -1 }), item({ cooldown: 0.5 }), item({ cooldown: 3601 }),
    item({ charges: 1000 }), item({ can_cast: 1 }), item({ passive: "false" }), null]) {
    const snapshot = input(); snapshot.own.fields.items.value = [item({ slot: "inventory1", name: null, cooldown: null }), bad];
    const result = parsed(snapshot);
    assert.deepEqual(result.own?.fields.items, { value: null, availability: "invalid" });
    assert.equal(result.own?.fields.hp.availability, "fresh");
    assert.equal(result.own?.fields.abilities?.availability, "fresh");
  }
});

test("invalid abilities, duplicate slots and excessive arrays are invalid-safe", () => {
  for (const bad of [ability({ slot: -1 }), ability({ slot: 8 }), ability({ name: null }), ability({ name: "Bad-ID" }),
    ability({ level: -1 }), ability({ level: 31 }), ability({ cooldown: Infinity }), ability({ active: 0 }), ability({ ultimate: "yes" })]) {
    const snapshot = input(); snapshot.own.fields.abilities.value = [bad];
    assert.deepEqual(parsed(snapshot).own?.fields.abilities, { value: null, availability: "invalid" });
  }
  const duplicate = input(); duplicate.own.fields.items.value = [item(), item()];
  assert.equal(parsed(duplicate).own?.fields.items?.availability, "invalid");
  const tooMany = input(); tooMany.own.fields.abilities.value = Array.from({ length: 9 }, (_, slot) => ability({ slot }));
  assert.equal(parsed(tooMany).own?.fields.abilities?.availability, "invalid");
});

test("all eleven item slots and optional null metadata parse, without manufacturing empty slots", () => {
  const snapshot = input(); snapshot.own.fields.items.value = ITEM_SLOTS.map(slot => ({ slot, name: null, cooldown: null, charges: null, can_cast: null, passive: null }));
  const result = parsed(snapshot);
  assert.equal(result.own?.fields.items?.value?.length, 11);
  assert.equal(result.own?.fields.items?.value?.[0]?.cooldown, null);
  assert.equal(result.own?.fields.items?.value?.[0]?.charges, null);
  assert.equal(result.own?.fields.items?.value?.[0]?.can_cast, null);
  snapshot.own.fields.items.value = [item({ cooldown: 3600, charges: 999 })];
  assert.equal(parsed(snapshot).own?.fields.items?.availability, "fresh");
  // The host always sends every key: a missing scalar or a fresh null list is invalid, not unknown.
  snapshot.own.fields.items.value = [{ slot: "inventory0", name: null }];
  assert.equal(parsed(snapshot).own?.fields.items?.availability, "invalid");
  snapshot.own.fields.items = { value: null, availability: "fresh" };
  assert.equal(parsed(snapshot).own?.fields.items?.availability, "invalid");
});

test("malformed collection envelopes and contradictory freshness do not discard valid hero data", () => {
  for (const field of [{ value: {}, availability: "fresh" }, { value: [item()], availability: "unknown" },
    { value: [item()], availability: "stale" }, { value: null, availability: "invented" }]) {
    const snapshot = input(); snapshot.own.fields.items = field;
    assert.equal(parsed(snapshot).own?.fields.items?.availability, "invalid");
  }
});

test("item names keep raw ids, aliases and recipes; unknown items use readable fallback names", () => {
  for (const [id, name] of [["item_blink", "Blink Dagger"], ["item_ultimate_scepter_2", "Aghanim's Scepter"],
    ["item_cyclone", "Eul's Scepter of Divinity"], ["item_recipe_blink", "Recipe"], ["item_future_test_item", "Future Test Item"],
    ["item_black_king_bar", "Black King Bar"], ["item_tpscroll", "Town Portal Scroll"], ["item_travel_boots", "Boots of Travel"],
    ["item_power_treads", "Power Treads"], ["item_magic_wand", "Magic Wand"], ["item_ultimate_scepter", "Aghanim's Scepter"],
    ["item_aghanims_shard", "Aghanim's Shard"], ["item_sheepstick", "Scythe of Vyse"], ["item_heart", "Heart of Tarrasque"],
    ["item_greater_crit", "Daedalus"], ["item_lifesteal", "Morbid Mask"], ["item_force_staff", "Force Staff"],
    ["item_glimmer_cape", "Glimmer Cape"], ["item_mjollnir", "Mjollnir"], ["item_bfury", "Battle Fury"]]) {
    assert.deepEqual(displayItemName(id!), { name, rawId: id });
  }
  assert.equal(displayItemName(null), null); assert.equal(displayItemName(""), null);
  assert.deepEqual(displayItemName("malformed"), { name: "malformed", rawId: "malformed" });
});

test("ability names strip the actual hero prefix and preserve the raw id", () => {
  assert.deepEqual(displayAbilityName("lion_impale"), { name: "Impale", rawId: "lion_impale" });
  assert.equal(displayAbilityName("crystal_maiden_crystal_nova", "npc_dota_hero_crystal_maiden")?.name, "Crystal Nova");
  assert.equal(displayAbilityName("future_hero_new_spell", "npc_dota_hero_future_hero")?.name, "New Spell");
  assert.equal(displayAbilityName("generic_spell", "npc_dota_hero_lion")?.name, "Generic Spell");
  assert.equal(displayAbilityName(null), null); assert.equal(displayAbilityName(""), null);
});

test("fresh empty slots differ from omitted slots and unknown fields", () => {
  const view = deriveLoadoutViewModel(parsed());
  assert.equal(view.items.slots?.[0]?.kind, "item"); assert.equal(view.items.slots?.[0]?.name, "Blink Dagger");
  assert.equal(view.items.slots?.[1]?.kind, "empty"); assert.equal(view.items.slots?.[2]?.kind, "unknown");
  const snapshot = input(); delete snapshot.own.fields.items;
  assert.equal(deriveLoadoutViewModel(parsed(snapshot)).items.slots, null);
});

test("cooldown and charge badges appear only for observed positive numbers", () => {
  const snapshot = input();
  for (const cooldown of [null, 0, 7]) for (const charges of [null, 0, 3]) {
    snapshot.own.fields.items.value = [item({ cooldown, charges })];
    const slot = deriveLoadoutViewModel(parsed(snapshot)).items.slots?.[0];
    assert.equal(slot?.cooldown, cooldown === 7 ? 7 : null); assert.equal(slot?.charges, charges === 3 ? 3 : null);
  }
});

test("abilities sort by slot, exclude passive/unknown-passive entries, and bound level pips", () => {
  const snapshot = input(); snapshot.own.fields.abilities.value = [ability({ slot: 5, ultimate: true, level: 8 }),
    ability({ slot: 2, passive: true }), ability({ slot: 3, passive: null }), ability({ slot: 0, level: 8 })];
  const tiles = deriveLoadoutViewModel(parsed(snapshot)).abilities.tiles!;
  assert.deepEqual(tiles.map(tile => tile.slot), [0, 5]);
  assert.equal(tiles[0]?.pips.length, 4); assert.equal(tiles[1]?.pips.length, 6); assert.equal(tiles[1]?.ultimate, true);
  snapshot.own.fields.abilities.value = [ability({ ultimate: null, level: 2 })];
  assert.equal(deriveLoadoutViewModel(parsed(snapshot)).abilities.tiles?.[0]?.pips, null, "unknown ultimate flag cannot imply a four-rank capacity");
});

test("ready text requires can_cast true and a positive cooldown takes precedence", () => {
  for (const can_cast of [null, false, true]) for (const cooldown of [null, 0, 7]) {
    const snapshot = input(); snapshot.own.fields.abilities.value = [ability({ can_cast, cooldown, level: null, ultimate: null })];
    const tile = deriveLoadoutViewModel(parsed(snapshot)).abilities.tiles?.[0];
    assert.equal(tile?.status, cooldown === 7 ? "cooldown" : can_cast === true ? "ready" : "unknown");
    assert.equal(abilityStatusText(tile!, "th"), cooldown === 7 ? "7s" : can_cast === true ? "พร้อม" : "—");
    assert.equal(tile?.pips, null);
    assert.equal(tile?.ultimate, null, "unknown ultimate metadata is not false");
  }
});

test("stale loadouts keep old age and dimmed dashes instead of ready or empty claims", () => {
  const snapshot = input(); snapshot.availability = "stale"; snapshot.source.age_ms = 3200;
  for (const field of Object.values(snapshot.own.fields) as any[]) if (field.availability === "fresh") field.availability = "stale";
  const view = deriveLoadoutViewModel(parsed(snapshot));
  assert.equal(view.items.stale, true); assert.equal(view.abilities.stale, true);
  assert.equal(view.items.ageMs, 3200); assert.equal(view.items.slots, null); assert.equal(view.abilities.tiles, null);
  const lost = deriveLoadoutViewModel(parsed(), true);
  assert.equal(lost.items.slots, null); assert.equal(lost.items.stale, true); assert.equal(lost.items.ageMs, null);
});

test("setup hint appears only for legacy or unknown loadout with other fresh fields", () => {
  assert.equal(deriveLoadoutViewModel(parsed()).setupHint, false);
  for (const version of ["2.0.0", "2.1.0", "2.2.0"]) {
    const snapshot = input(); snapshot.schema_version = version; delete snapshot.own.fields.items;
    assert.equal(deriveLoadoutViewModel(parsed(snapshot)).setupHint, true);
  }
  const snapshot = input(); snapshot.own.fields.items = { value: null, availability: "invalid" };
  assert.equal(deriveLoadoutViewModel(parsed(snapshot)).setupHint, false);
  snapshot.own.fields.items = { value: null, availability: "unsupported" };
  assert.equal(deriveLoadoutViewModel(parsed(snapshot)).setupHint, false);
  snapshot.own.fields.items = { value: null, availability: "unknown" };
  for (const field of Object.values(snapshot.own.fields) as any[]) { field.value = null; field.availability = "unknown"; }
  assert.equal(deriveLoadoutViewModel(parsed(snapshot)).setupHint, false);
  assert.equal(deriveLoadoutViewModel(null).setupHint, false);
});

test("development sample is 2.2, parses through the real boundary and stays independent of base fixtures", () => {
  const base = input(); const sample = addSampleLoadout(parsed(base));
  assert.equal(sample.schema_version, "2.2.0"); assert.ok(parseOwnSnapshot(sample));
  assert.ok(sample.own?.fields.items?.value?.some(entry => entry.name === null));
  assert.notEqual(sample.own?.fields.items?.value, base.own.fields.items.value);
});

test("loadout tiles remain compact and the root/card never gain scrolling", () => {
  const css = readFileSync(new URL("../src/deck/loadout.css", import.meta.url), "utf8");
  assert.doesNotMatch(css, /overflow(?:-y)?:\s*(auto|scroll)/);
  assert.match(css, /\.deck-inventory\s*\{[^}]*repeat\(6,\s*minmax\(0,\s*1fr\)\)/);
  assert.match(css, /\.deck-ability-grid\s*\{[^}]*repeat\(4,\s*minmax\(0,\s*1fr\)\)/);
  assert.match(css, /text-overflow:\s*ellipsis/); assert.match(css, /\.deck-loadout-section\.is-stale\s*\{[^}]*opacity/);
  assert.doesNotMatch(css, /#[0-9a-f]{3,8}\b|rgba?\(|--ally|--accent/i);
  const component = readFileSync(new URL("../src/deck/OwnLoadout.tsx", import.meta.url), "utf8");
  assert.equal(component.match(/ต้องตั้งค่าการเชื่อมต่อ Dota 2 ใหม่หนึ่งครั้ง/g)?.length, 1);
  const deck = readFileSync(new URL("../src/deck/Deck.tsx", import.meta.url), "utf8");
  assert.match(deck, /<details className="deck-ally-disclosure"/);
});
