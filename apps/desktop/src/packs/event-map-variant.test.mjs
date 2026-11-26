import assert from "node:assert/strict";
import { test } from "node:test";
import { parseEventMapVariant } from "./event-map-variant.ts";

const variant = {
  variant_id: "clip_b",
  clip_id: "vrzo-th-0024",
  duration_ms: 2_350,
  has_visual: true,
};

test("event-map 1.1.0 carries optional captions and preserves their original text", () => {
  for (const caption of ["ต่อไปใครจะโดน", "e\u0301", "🦋".repeat(200)]) {
    const row = { ...variant, caption };
    assert.deepEqual(parseEventMapVariant(row, "1.1.0"), row);
  }
  assert.deepEqual(parseEventMapVariant(variant, "1.1.0"), variant);
});

test("event-map 1.0.0 accepts caption-free rows", () => {
  assert.deepEqual(parseEventMapVariant(variant, "1.0.0"), variant);
  assert.equal(parseEventMapVariant({ ...variant, caption: "voice" }, "1.0.0"), null);
});

test("event-map 1.1.0 rejects malformed captions without blocking other variants", () => {
  for (const caption of [
    null, 12, "", " leading", "trailing\u00a0", "🦋".repeat(201), "a\u0000b",
    "a\u007fb", "a\tb", "a\nb", "a\rb", "a\u0085b", "a\u2028b", "a\u2029b", "a\ud800b",
  ]) {
    assert.equal(parseEventMapVariant({ ...variant, caption }, "1.1.0"), null, JSON.stringify(caption));
  }
  assert.deepEqual(parseEventMapVariant(variant, "1.1.0"), variant);
});

test("event-map variant parser still validates the rest of each payload", () => {
  const { clip_id, ...withoutClipId } = variant;
  assert.deepEqual(parseEventMapVariant(withoutClipId, "1.1.0"), withoutClipId);
  for (const fields of [
    { variant_id: "invalid id" }, { clip_id: "" }, { duration_ms: -1 },
    { duration_ms: 15_001 }, { has_visual: "true" }, { extra: true },
  ]) {
    assert.equal(parseEventMapVariant({ ...variant, ...fields }, "1.1.0"), null);
  }
});

test("event-map 1.3.0 exposes validated hero-only binding conditions", () => {
  const row = {
    ...variant, install_id: "ins_0000000001", package_id: "voice.sniper", equipped_slot: 0,
    state: "ready", reason: "NONE", hero: "npc_dota_hero_sniper",
  };
  assert.deepEqual(parseEventMapVariant(row, "1.3.0"), row);
  assert.equal(parseEventMapVariant(row, "1.2.0"), null);
  for (const hero of ["sniper", "npc_dota_hero_", "npc_dota_hero_Sniper", null]) {
    assert.equal(parseEventMapVariant({ ...variant, hero }, "1.3.0"), null);
  }
});
