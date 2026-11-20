import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import { test } from "node:test";
import "../src/packs/event-map-variant.test.mjs";

const model = await import("../src/packs/variant-model.ts");
const copyModule = await import("../src/packs/event-variants.copy.ts");

const baseVariant = {
  variant_id: "clip_b",
  clip_id: "vrzo-th-0024",
  caption: "ต่อไปใครจะโดน",
  duration_ms: 2_350,
  has_visual: true,
};

test("variant row model includes caption, duration, and banner state", () => {
  const copy = copyModule.eventVariantsCopy("th");
  assert.deepEqual(model.variantRowPresentation(baseVariant, copy), {
    captionText: "ต่อไปใครจะโดน",
    durationText: "2.4 วินาที",
    bannerText: "มีแบนเนอร์",
  });
  assert.deepEqual(model.variantRowPresentation({ ...baseVariant, has_visual: false }, copy), {
    captionText: "ต่อไปใครจะโดน",
    durationText: "2.4 วินาที",
    bannerText: "ไม่มีแบนเนอร์",
  });
});

test("EventVariants renders one VariantRow for each variant with the row fields and play action", async () => {
  const panelSource = await readFile(new URL("../src/packs/EventVariants.tsx", import.meta.url), "utf8");
  const rowSource = await readFile(new URL("../src/packs/VariantRow.tsx", import.meta.url), "utf8");
  assert.match(panelSource, /variants\.map\(\(variant\) => \(/);
  assert.match(panelSource, /<VariantRow[\s\S]*?variant=\{variant\}/);
  assert.match(rowSource, /presentation\.captionText/);
  assert.match(rowSource, /presentation\.durationText/);
  assert.match(rowSource, /presentation\.bannerText/);
  assert.match(rowSource, /onClick=\{\(\) => onAudition\(variant\.variant_id\)\}/);
  assert.match(rowSource, /disabled=\{disabled \|\| busy\}/);
});

test("caption stays optional and selection labels distinguish random and ordered playback", () => {
  const copy = copyModule.eventVariantsCopy("en");
  assert.equal(model.variantRowPresentation({ ...baseVariant, caption: undefined }, copy).captionText, "No caption supplied");
  assert.equal(model.variantSelectionLabel("random", copy), "Random");
  assert.equal(model.variantSelectionLabel("shuffle", copy), "Shuffle without repeats");
  assert.equal(model.variantSelectionLabel("sequence", copy), "In order");
  assert.equal(model.variantSelectionLabel(null, copy), null);
});

test("variant audition outcome names audio and each visual result", () => {
  const copy = copyModule.eventVariantsCopy("en");
  const result = {
    audio_started: true,
    audio_ended: true,
    audio_end_reason: "finished",
    visual_status: "shown" as const,
    visual_reason: null,
    video_error_code: null,
  };
  assert.equal(model.auditionOutcomeText(result, copy), "Audio ended · Banner shown");
  assert.equal(
    model.auditionOutcomeText({ ...result, visual_status: "failed" }, copy),
    "Audio ended · Banner failed",
  );
  assert.equal(
    model.auditionOutcomeText({ ...result, visual_status: "none" }, copy),
    "Audio ended · No banner for this clip",
  );
  assert.equal(
    model.auditionOutcomeText({ ...result, audio_started: false, visual_status: "unknown" }, copy),
    "Audio did not start · Banner outcome unknown",
  );
});
