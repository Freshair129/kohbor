import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { readFileSync } from "node:fs";
import { test } from "node:test";
import { chooseMapCalibration, isControlEffect, isUltimateAbility, replayPackForPatch } from "../src/replay/replayPack.mjs";
import type { ReplayPatchData } from "../src/replay/replayPack.mjs";

const readPack = (patch: string) => JSON.parse(readFileSync(new URL(`../src/assets/replay-packs/${patch}/replay-data.json`, import.meta.url), "utf8")) as ReplayPatchData;
const hash = (bytes: Uint8Array) => createHash("sha256").update(bytes).digest("hex");

test("patch lookup supports only generated packs and marks unknown patches needs_pack", () => {
  assert.equal(replayPackForPatch("7.41f").state, "ready");
  assert.equal(replayPackForPatch("7.42").state, "ready");
  assert.deepEqual(replayPackForPatch("7.43"), { state: "needs_pack", patch: "7.43" });
  assert.deepEqual(replayPackForPatch(null), { state: "needs_pack", patch: null });
});

test("manifest calibration wins; matching pack calibration is the fallback", () => {
  const pack = readPack("7.42");
  const fromManifest = chooseMapCalibration({ px: [1.25, 20], py: [-2, 30] }, 512, pack.map);
  assert.deepEqual(fromManifest, { calibration: { size: 512, a: 1.25, b: 20, c: -2, d: 30 }, source: "manifest" });
  const fromPack = chooseMapCalibration(null, null, pack.map);
  assert.deepEqual(fromPack, { calibration: { size: pack.map.size, a: pack.map.calibration.px[0], b: pack.map.calibration.px[1], c: pack.map.calibration.py[0], d: pack.map.calibration.py[1] }, source: "pack" });
  assert.deepEqual(chooseMapCalibration(null, null, undefined), { calibration: null, source: "unknown" });
});

test("generated CC ability subsets match dotaconstants source descriptions for three heroes", () => {
  const sourceDescriptions = {
    axe_berserkers_call: "Axe taunts nearby enemy units, forcing them to attack him while he gains bonus armor during the duration.",
    lion_impale: "Rock spikes burst from the earth along a straight path. Enemy units are hurled into the air, then are stunned and take damage when they fall.",
    lion_voodoo: "Transforms an enemy unit into a harmless beast, with all special abilities disabled.",
    shadow_shaman_shackles: "CHANNELED - Magically binds an enemy unit so that it cannot move or attack, absorbing their life energy over time.",
  };
  for (const patch of ["7.41f", "7.42"]) {
    const pack = readPack(patch);
    for (const [ability, description] of Object.entries(sourceDescriptions)) {
      assert.equal(pack.control_ability_source[ability]?.desc, description, `${patch}:${ability} source subset`);
      assert.ok(pack.ability_names_by_hero[ability.split("_").slice(0, ability.startsWith("shadow_shaman") ? 2 : 1).join("_")]?.includes(ability));
    }
    assert.ok(pack.control_abilities_by_hero.axe.includes("axe_berserkers_call"), `${patch}: axe taunt`);
    assert.ok(pack.control_abilities_by_hero.lion.includes("lion_impale"), `${patch}: lion stun`);
    assert.ok(pack.control_abilities_by_hero.lion.includes("lion_voodoo"), `${patch}: lion hex`);
    assert.ok(pack.control_abilities_by_hero.shadow_shaman.includes("shadow_shaman_shackles"), `${patch}: shadow shaman bind`);
    assert.equal(isControlEffect(pack, "modifier_axe_berserkers_call"), true);
  }
});

test("pack provenance records source commit, source hashes and output hash under the size budget", () => {
  let totalBytes = 0;
  for (const patch of ["7.41f", "7.42"]) {
    const dataPath = new URL(`../src/assets/replay-packs/${patch}/replay-data.json`, import.meta.url);
    const dataBytes = new Uint8Array(readFileSync(dataPath));
    const provenance = JSON.parse(readFileSync(new URL(`../src/assets/replay-packs/${patch}/provenance.json`, import.meta.url), "utf8"));
    totalBytes += dataBytes.byteLength + Buffer.byteLength(JSON.stringify(provenance, null, 2) + "\n");
    assert.equal(provenance.engine_source_commit, "a609ecc9d5221dcf62acd9c0312ce7cb8d8f79f9");
    assert.match(provenance.dotaconstants_version, /dotaconstants/);
    assert.match(provenance.license_note, /MIT/);
    assert.equal(provenance.generated_files[0].sha256, hash(dataBytes));
    assert.ok(provenance.source_files.every((file: { sha256: string }) => /^[0-9a-f]{64}$/.test(file.sha256)));
  }
  assert.ok(totalBytes < 2 * 1024 * 1024);
});

test("source pack has no verified hero-to-ultimate slots, so the viewer does not invent a list", () => {
  const pack = readPack("7.42");
  assert.equal(pack.ultimate_classification.available, false);
  assert.equal(pack.ultimate_classification.abilities_by_hero, null);
  assert.equal(isUltimateAbility(pack, "npc_dota_hero_lion", "lion_finger_of_death"), false);
});
