import assert from "node:assert/strict";
import { registerHooks } from "node:module";
import { test } from "node:test";

// The renderer uses Vite's extensionless imports; mirror that resolution for
// source imports while keeping this test on Node's native TypeScript loader.
registerHooks({
  resolve(specifier, context, nextResolve) {
    if (context.parentURL?.includes("/apps/desktop/src/")
      && specifier.startsWith(".")
      && !/\.[a-z0-9]+$/iu.test(specifier)) {
      return nextResolve(`${specifier}.ts`, context);
    }
    return nextResolve(specifier, context);
  },
});

const {
  parseMediaAuditionPlaybackEvent,
  parseMediaPackSummaries,
  parseMediaVariantVisualUrl,
} = await import("../src/runtime.ts");

const installId = "ins_0000000001";
const legacyRecord = {
  install_id: installId,
  package_id: "voice-pack",
  revision: 4,
  active: false,
  pending: false,
  display_name: "Legacy pack",
  problem: null,
  cover_url: null,
  event_count: 1,
  binding_count: 1,
  variant_count: 1,
  banner_count: 0,
  bindings: [{ event_id: "hero_kill", tier: null, selection: null, variant_count: 1, banner_count: 0 }],
};

const coverUrl = `kohbor-media://localhost/v1/cover/${installId}/${"a".repeat(64)}`;
const currentRecord = {
  ...legacyRecord,
  cover_url: coverUrl,
  authority_class: "local_import",
  publisher_claim: "author-key-claim",
  installed_at: 1_795_000_000_000,
  size_bytes: 4096,
  game_id: "dota2",
  presentation: {
    description: "Pack description",
    author: "Pack author",
    languages: ["en-US", "th-TH"],
    cover_url: coverUrl,
  },
};

test("media pack summary parser keeps accepting 1.0.0 rows", () => {
  const parsed = parseMediaPackSummaries({ schema_version: "1.0.0", records: [legacyRecord] });
  assert.equal(parsed?.schema_version, "1.0.0");
  assert.equal(parsed?.records[0]?.install_id, installId);
});

test("1.1.0 summaries carry host authority, nullable claims, metadata and safe presentation", () => {
  const parsed = parseMediaPackSummaries({ schema_version: "1.1.0", records: [currentRecord] });
  assert.equal(parsed?.schema_version, "1.1.0");
  const record = parsed?.records[0];
  assert.equal(record?.authority_class, "local_import");
  assert.equal(record?.publisher_claim, "author-key-claim");
  assert.equal(record?.installed_at, 1_795_000_000_000);
  assert.equal(record?.size_bytes, 4096);
  assert.equal(record?.game_id, "dota2");
  assert.equal(record?.presentation?.cover_url, coverUrl);
});

test("1.1.0 parser rejects invalid authority, path leaks, unsafe cover URLs and malformed languages", () => {
  assert.equal(parseMediaPackSummaries({
    schema_version: "1.1.0", records: [{ ...currentRecord, authority_class: "system-looking-name" }],
  }), null);
  assert.equal(parseMediaPackSummaries({
    schema_version: "1.1.0", records: [{ ...currentRecord, presentation: { ...currentRecord.presentation, cover: "C:\\secret.png" } }],
  }), null);
  assert.equal(parseMediaPackSummaries({
    schema_version: "1.1.0", records: [{ ...currentRecord, cover_url: coverUrl.replace("ins_0000000001", "ins_0000000002") }],
  }), null);
  assert.equal(parseMediaPackSummaries({
    schema_version: "1.1.0", records: [{ ...currentRecord, presentation: { ...currentRecord.presentation, languages: ["en-12"] } }],
  }), null);
});

test("drawer visual URL parser accepts only a host media route and no filesystem path", () => {
  const visualUrl = `kohbor-media://localhost/v1/${installId}/${"b".repeat(64)}`;
  assert.deepEqual(parseMediaVariantVisualUrl({ visual_url: visualUrl }), { visual_url: visualUrl });
  assert.deepEqual(parseMediaVariantVisualUrl({ visual_url: null }), { visual_url: null });
  assert.equal(parseMediaVariantVisualUrl({ visual_url: "C:\\packs\\clip.webm" }), null);
  assert.equal(parseMediaVariantVisualUrl({ visual_url: "https://example.invalid/clip.webm" }), null);
  assert.equal(parseMediaVariantVisualUrl({ visual_url: visualUrl, path: "clip.webm" }), null);
});

test("audition lifecycle parser accepts exact playback events and rejects unrelated or unverified shapes", () => {
  const started = {
    kind: "started", playback_id: "pb_0000000001", class: "ordinary", event_type: "audition",
    tier: 2, visual_url: null, audio_duration_ms: 1250, visual_tail_ms: 0, at_ms: 500,
  };
  const ended = { kind: "ended", playback_id: "pb_0000000001", reason: "finished", at_ms: 1750 };
  assert.equal(parseMediaAuditionPlaybackEvent(started)?.event_type, "audition");
  assert.deepEqual(parseMediaAuditionPlaybackEvent(ended), ended);
  assert.equal(parseMediaAuditionPlaybackEvent({ ...started, event_type: "hero_kill" }), null);
  assert.equal(parseMediaAuditionPlaybackEvent({ ...started, visual_url: "C:\\packs\\clip.webm" }), null);
  assert.equal(parseMediaAuditionPlaybackEvent({ ...ended, reason: "unknown" }), null);
});
