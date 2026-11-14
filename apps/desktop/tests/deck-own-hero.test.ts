import assert from "node:assert/strict";
import { test } from "node:test";
// @ts-ignore Node's built-in TypeScript loader requires the explicit source extension.
import { deriveOwnHeroViewModel, ownHeroLifeLabel } from "../src/deck/ownHeroModel.ts";
// @ts-ignore Node's built-in TypeScript loader requires the explicit source extension.
import type { Field, FieldAvailability, OwnSnapshot } from "../src/types.ts";

function field<T>(value: T | null, availability: FieldAvailability = "fresh"): Field<T> {
  return { value, availability };
}

function snapshot(overrides: Partial<NonNullable<OwnSnapshot["own"]>["fields"]> = {}, availability: FieldAvailability = "fresh"): OwnSnapshot {
  return {
    schema_version: "2.0.0",
    session_epoch: 1,
    availability,
    source: { kind: "gsi", generation: 1, sequence: 2, age_ms: 640 },
    own: {
      identity_revision: 1,
      team: "radiant",
      fields: {
        hero_id: field("npc_dota_hero_axe"),
        level: field(12),
        hp: field(400),
        max_hp: field(800),
        mana_percent: field(37),
        alive: field(true),
        respawn_seconds: field(0),
        ...overrides,
      },
    },
  };
}

test("fresh own hero fields are projected without translating the hero id", () => {
  const result = deriveOwnHeroViewModel(snapshot());
  assert.equal(result.heroId, "npc_dota_hero_axe");
  assert.equal(result.level, 12);
  assert.equal(result.hp, 400);
  assert.equal(result.maxHp, 800);
  assert.equal(result.hpPercent, 50);
  assert.equal(result.manaPercent, 37);
  assert.equal(result.alive, true);
  assert.equal(result.ageMs, 640);
  assert.equal(result.hasOwnSnapshot, true);
});

test("non-fresh fields become unknown values while the last snapshot age remains visible", () => {
  const result = deriveOwnHeroViewModel(snapshot({
    hero_id: field("npc_dota_hero_axe", "stale"),
    level: field(12, "unknown"),
    hp: field(400, "invalid"),
    max_hp: field(800, "unsupported"),
    mana_percent: field(37, "stale"),
    alive: field(false, "unknown"),
    respawn_seconds: field(8, "stale"),
  }, "stale"));
  assert.equal(result.heroId, null);
  assert.equal(result.level, null);
  assert.equal(result.hp, null);
  assert.equal(result.maxHp, null);
  assert.equal(result.hpPercent, null);
  assert.equal(result.manaPercent, null);
  assert.equal(result.alive, null);
  assert.equal(result.respawnSeconds, null);
  assert.equal(result.ageMs, 640);
});

test("unknown snapshot availability does not leave an age chip looking current", () => {
  assert.equal(deriveOwnHeroViewModel(snapshot({}, "unknown")).ageMs, null);
});

test("HP percentage comes from fresh HP values and is a dash when max HP is zero", () => {
  assert.equal(deriveOwnHeroViewModel(snapshot({ hp: field(200), max_hp: field(0) })).hpPercent, null);
  assert.equal(deriveOwnHeroViewModel(snapshot({ hp: field(200), max_hp: field(400, "stale") })).hpPercent, null);
});

test("respawn seconds are shown when fresh and remain dashed otherwise", () => {
  const dead = deriveOwnHeroViewModel(snapshot({ alive: field(false), respawn_seconds: field(7) }));
  assert.equal(ownHeroLifeLabel(dead, "th"), "เกิดใหม่ใน 7 วินาที");
  const unknown = deriveOwnHeroViewModel(snapshot({ alive: field(false), respawn_seconds: field(7, "stale") }));
  assert.equal(ownHeroLifeLabel(unknown, "th"), "เกิดใหม่ใน — วินาที");
});

test("missing snapshot or own block does not invent a hero or latest-result card", () => {
  assert.equal(deriveOwnHeroViewModel(null).heroId, null);
  assert.equal(deriveOwnHeroViewModel(null).hasOwnSnapshot, false);
  const noOwn = snapshot();
  noOwn.own = null;
  assert.equal(deriveOwnHeroViewModel(noOwn).hasOwnSnapshot, false);
  assert.equal(deriveOwnHeroViewModel(noOwn).hpPercent, null);
});
