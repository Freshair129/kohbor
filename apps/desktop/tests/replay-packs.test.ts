import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { readFileSync } from "node:fs";
import { test } from "node:test";
import { chooseMapCalibration, installedAbilityData, isControlEffect, isUltimateAbility, replayPackForPatch, worldRadiusToMapPixels } from "../src/replay/replayPack.mjs";
import type { ReplayPatchData } from "../src/replay/replayPack.mjs";

const readPack = (patch: string) => JSON.parse(readFileSync(new URL(`../src/assets/replay-packs/${patch}/replay-data.json`, import.meta.url), "utf8")) as ReplayPatchData;
const hash = (bytes: Uint8Array) => createHash("sha256").update(bytes).digest("hex");

test("patch lookup supports only generated packs and marks unknown patches needs_pack", () => {
  assert.equal(replayPackForPatch("7.41f").state, "ready");
  assert.equal(replayPackForPatch("7.42").state, "ready");
  assert.deepEqual(replayPackForPatch("7.43"), { state: "needs_pack", patch: "7.43" });
  assert.deepEqual(replayPackForPatch(null), { state: "needs_pack", patch: null });
});

test("the selected patch pack calibration is authoritative over manifest values", () => {
  const pack = readPack("7.42");
  const fromManifest = chooseMapCalibration({ px: [1.25, 20], py: [-2, 30] }, 512, pack.map);
  const fromPack = chooseMapCalibration(null, null, pack.map);
  assert.deepEqual(fromManifest, fromPack);
  assert.deepEqual(fromPack, { calibration: { size: pack.map.size, a: pack.map.calibration.px[0], b: pack.map.calibration.px[1], c: pack.map.calibration.py[0], d: pack.map.calibration.py[1] }, source: "pack" });
  assert.deepEqual(chooseMapCalibration(null, null, undefined), { calibration: null, source: "unknown" });
});

test("registered 7.42 overview calibration preserves the official Radiant ancient projection", () => {
  const pack = readPack("7.42");
  assert.match(String(pack.map.calibration.method), /registered to the official overview/);
  const result = chooseMapCalibration(null, null, pack.map).calibration!;
  const candidatePx = result.a * -5920 + result.b;
  const candidatePy = result.c * -5352 + result.d;
  const provenance = JSON.parse(readFileSync(new URL("../src/assets/replay-packs/7.42/provenance.json", import.meta.url), "utf8"));
  const fit = provenance.map_asset.registration.candidate_to_reference_transform;
  const officialPx = fit.x.scale * candidatePx + fit.x.offset_px;
  const officialPy = fit.y.scale * candidatePy + fit.y.offset_px;
  assert.ok(Math.abs(candidatePx - 196.927924379677) < 0.001, `candidate px=${candidatePx}`);
  assert.ok(Math.abs(candidatePy - 802.774022545586) < 0.001, `candidate py=${candidatePy}`);
  assert.ok(Math.abs(officialPx - 192.0) < 0.1, `official-reference px=${officialPx}`);
  assert.ok(Math.abs(officialPy - 801.3) < 0.1, `official-reference py=${officialPy}`);
});

test("world-space radii use the same registered 7.42 scale as projected points", () => {
  const pack = readPack("7.42");
  const calibration = chooseMapCalibration(null, null, pack.map).calibration!;
  const expected = 185 * Math.sqrt(Math.abs(calibration.a * calibration.c));
  assert.ok(Math.abs(worldRadiusToMapPixels(185, calibration, 1024)! - expected) < 1e-9);
  assert.ok(Math.abs(worldRadiusToMapPixels(185, calibration)! - expected) < 1e-9);
  assert.ok(Math.abs(expected - 9.846396218984) < 1e-9);
  assert.equal(worldRadiusToMapPixels(-1, calibration), null);
});

test("7.42 pack provenance and PNG hash describe the detailed 1024px image", () => {
  const bytes = new Uint8Array(readFileSync(new URL("../src/assets/replay-packs/7.42/map.png", import.meta.url)));
  const provenance = JSON.parse(readFileSync(new URL("../src/assets/replay-packs/7.42/provenance.json", import.meta.url), "utf8"));
  assert.deepEqual([...bytes.slice(0, 8)], [137, 80, 78, 71, 13, 10, 26, 10]);
  assert.equal(hash(bytes), provenance.map_asset.image_sha256);
  assert.equal(hash(bytes), "447028ade5f1075f65e7dbc9fd1a4979fe948973c01f8c5cf2b403bbf3a38769");
  assert.equal(bytes.byteLength, provenance.map_asset.image_bytes);
  assert.equal(bytes.byteLength, 913042);
  assert.equal(provenance.map_asset.style, "detailed");
  assert.equal(provenance.map_asset.material_logical_path, "materials/overviews/dota.vmat");
  assert.equal(provenance.map_asset.simple_material_logical_path, "materials/overviews/dota_minimal.vmat");
  assert.equal(provenance.map_asset.map_entry_override, "materials/overviews/dota_psd_8b4a9409.vtex_c");
  assert.equal(provenance.map_asset.texture_source_path, "materials/overviews/dota_psd_8b4a9409.vtex_c");
  assert.equal(provenance.map_asset.texture_source_sha256, "a10430c9dab968ed2fdca0c0f18d1a3005f8771d2dc8ebe6eb9f69883adccff5");
  assert.deepEqual(provenance.map_asset.source_dimensions, [1024, 1024]);
  assert.deepEqual(provenance.map_asset.image_dimensions, [1024, 1024]);
  assert.match(provenance.map_asset.resampling, /source 1024 px, native-size export with no upscale or resampling/);
  assert.equal(provenance.map_asset.export_provenance.mapEntryOverride, provenance.map_asset.map_entry_override);
  assert.equal(provenance.map_asset.export_provenance.overview.textureReference.sha256, "6bd12841e16c747578faaee8aafdacea805a6cc8bfc5367eb08ad8fd58336196");
  assert.equal(provenance.map_asset.registration.edge_cross_correlation.identity, 0.495771448120488);
  assert.equal(provenance.map_asset.registration.edge_cross_correlation.fitted, 0.9706959281596982);
  assert.equal(provenance.map_asset.registration.decision.includes("exceed 2 px"), true);
  assert.equal(provenance.map_asset.registration.official_calibration_selected, false);
  assert.ok(provenance.map_asset.registration.official_mapping_residuals_px.every(
    (row: { official_delta_length_px: number }) => row.official_delta_length_px > 2,
  ));
  assert.deepEqual(readPack("7.42").map.calibration.px, provenance.map_asset.registered_runtime_calibration.px);
  assert.deepEqual(readPack("7.42").map.calibration.py, provenance.map_asset.registered_runtime_calibration.py);
  assert.deepEqual(provenance.map_asset.registration.official_mapping_residuals_px.map((row: { point: string }) => row.point), [
    "top_left", "top_right", "bottom_right", "bottom_left", "center",
  ]);
});

test("registered 7.42 calibration lands the fountain and Tormentor in measured map features", () => {
  const provenance = JSON.parse(readFileSync(new URL("../src/assets/replay-packs/7.42/provenance.json", import.meta.url), "utf8"));
  const landmarks = provenance.map_asset.registration.landmark_checks;
  const fountain = landmarks.find((row: { name: string }) => row.name === "radiant_fountain_feature_centroid");
  const tormentor = landmarks.find((row: { name: string }) => row.name === "tormentor_7744_-6208");
  assert.ok(fountain);
  assert.ok(tormentor);
  assert.equal(fountain.projection_inside_measured_feature, true);
  assert.ok(fountain.feature_center_residual_px < 0.5);
  assert.equal(fountain.candidate_projected_pixel_matches_feature_mask, null);
  assert.equal(tormentor.world_x, 7744);
  assert.equal(tormentor.world_y, -6208);
  assert.equal(tormentor.projection_inside_measured_feature, true);
  assert.equal(tormentor.candidate_projected_pixel_matches_feature_mask, true);
  assert.ok(tormentor.feature_center_residual_px < 8);
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

test("installed build 6989 ability slots generate the runtime ultimate and control lists", () => {
  const stats = JSON.parse(readFileSync(new URL("../src/assets/dota/6989/stats.json", import.meta.url), "utf8"));
  const sourcePack = readPack("7.42");
  const generated = installedAbilityData(stats, sourcePack);
  assert.equal(generated.ability_data_source?.build, 6989);
  assert.equal(generated.ultimate_classification.available, true);
  const lionUltimates = generated.ultimate_classification.abilities_by_hero?.lion ?? [];
  assert.ok(lionUltimates.includes("lion_finger_of_death"));
  assert.equal(isUltimateAbility({ ...readPack("7.42"), ...generated }, "npc_dota_hero_lion", "lion_finger_of_death"), true);

  const runtime = replayPackForPatch("7.42");
  assert.equal(runtime.state, "ready");
  if (runtime.state !== "ready") return;
  for (const [hero, ability] of [
    ["axe", "axe_berserkers_call"],
    ["lion", "lion_impale"],
    ["shadow_shaman", "shadow_shaman_shackles"],
  ] as const) {
    assert.ok(runtime.pack.ability_names_by_hero[hero]?.includes(ability), `${hero}:${ability} should come from installed build 6989 slots`);
    assert.ok(sourcePack.control_abilities_by_hero[hero]?.includes(ability), `${hero}:${ability} source semantics should match dotaconstants`);
    assert.ok(runtime.pack.control_abilities_by_hero[hero]?.includes(ability), `${hero}:${ability} should survive installed-slot filtering`);
  }
  assert.equal(runtime.pack.ability_data_source?.kind, "installed_game_build");
});
