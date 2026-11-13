import assert from "node:assert/strict";
import { test } from "node:test";

const status = await import("../src/mediaStatus.ts");
const copy = await import("../src/copy.ts");

const media = (state: string, reason: string | null) => ({ state, reason }) as never;

test("AUDIO_MUTED means muted, whatever was known before", () => {
  assert.equal(status.deriveMuted(media("unavailable", "AUDIO_MUTED"), false), true);
  assert.equal(status.deriveMuted(media("unavailable", "AUDIO_MUTED"), null), true);
});

test("ready and media reasons mean not muted, because the host tests mute first", () => {
  assert.equal(status.deriveMuted(media("ready", null), true), false);
  for (const reason of ["MEDIA_UNMAPPED", "MEDIA_INVALID", "MEDIA_PROBE_FAILED"]) {
    assert.equal(status.deriveMuted(media("not_ready", reason), true), false);
  }
});

test("CAPABILITY_UNAVAILABLE keeps the last known value and never invents one", () => {
  assert.equal(status.deriveMuted(media("unavailable", "CAPABILITY_UNAVAILABLE"), true), true);
  assert.equal(status.deriveMuted(media("unavailable", "CAPABILITY_UNAVAILABLE"), false), false);
  assert.equal(status.deriveMuted(media("unavailable", "CAPABILITY_UNAVAILABLE"), null), null);
  assert.equal(status.deriveMuted(media("unavailable", "SOMETHING_NEW"), null), null);
  assert.equal(status.deriveMuted(null, true), true);
});

test("media tone never reads as fresh unless the host says ready", () => {
  assert.equal(status.mediaTone(null), "unknown");
  assert.equal(status.mediaTone(media("ready", null)), "fresh");
  assert.equal(status.mediaTone(media("not_ready", "MEDIA_UNMAPPED")), "warn");
  assert.equal(status.mediaTone(media("unavailable", "AUDIO_MUTED")), "off");
});

test("media reasons are worded for audio, not as a missing build feature", () => {
  for (const locale of ["th", "en"] as const) {
    assert.notEqual(copy.mediaReason("CAPABILITY_UNAVAILABLE", locale), copy.reason("CAPABILITY_UNAVAILABLE", locale));
    assert.equal(copy.mediaReason("MEDIA_UNMAPPED", locale), copy.reason("MEDIA_UNMAPPED", locale));
    assert.equal(copy.mediaReason(null, locale), "");
    for (const code of ["MEDIA_UNMAPPED", "MEDIA_INVALID", "MEDIA_PROBE_FAILED", "AUDIO_MUTED", "CAPABILITY_UNAVAILABLE"]) {
      assert.ok(copy.mediaHint(code, locale), `hint for ${code} in ${locale}`);
    }
    assert.equal(copy.mediaHint("SOMETHING_NEW", locale), null);
    assert.equal(copy.mediaHint(null, locale), null);
  }
});
