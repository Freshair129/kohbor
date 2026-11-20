import assert from "node:assert/strict";
import { test } from "node:test";
import { parseBrowseEndedEvent, parseBrowsePackDetail, parseBrowseStartedEvent, type BrowsePackSummary } from "../src/packs/browseApi.ts";
import { parsePackSummaries } from "../src/packs/model.ts";
import { filterBrowsePacks, countPackOrigins } from "../src/packs/browseModel.ts";
import { createBrowsePreviewState, reduceBrowsePreview } from "../src/packs/previewModel.ts";
import { createBrowseApi } from "../src/packs/browseApi.ts";
import { gameMenuItems, unsupportedGameExplanation } from "../src/shell/game-selector.ts";
import { DEFAULT_GLASS_LEVEL, readGlassLevel, writeGlassLevel, solidGlassFallback } from "../src/shell/glassPreference.ts";

const installId = "ins_0000000001";
const sha = "a".repeat(64);
const summaryRecord = {
  install_id: installId,
  package_id: "voice.pack",
  revision: 2,
  active: false,
  pending: false,
  display_name: "เสียงสนาม",
  problem: null,
  cover_url: null,
  event_count: 1,
  binding_count: 1,
  variant_count: 1,
  banner_count: 1,
  bindings: [{ event_id: "hero_kill", tier: null, selection: null, variant_count: 1, banner_count: 1 }],
};

test("1.0.0 summaries keep authority unknown even if an old payload has an origin-looking field", () => {
  const [pack] = parsePackSummaries({ schema_version: "1.0.0", records: [{ ...summaryRecord, authority_class: "system" }] })!;
  assert.equal(pack?.authorityClass, "unknown");
  assert.equal(pack?.presentation.description, null);
  assert.equal(pack?.publisherClaim, null);
});

test("browse adapter keeps 1.0.0 authority explicitly unknown until the host supplies 1.1.0", async () => {
  const api = createBrowseApi({
    summaries: async () => ({ schema_version: "1.0.0", records: [{ ...summaryRecord, authority_class: "system" }] }),
    detail: async () => null, visualUrl: async () => null, audition: async () => null,
    subscribeStarted: async () => () => {}, subscribeEnded: async () => () => {},
  });
  const [pack] = await api.summaries() ?? [];
  assert.equal(pack?.authorityClass, "unknown");
});

test("1.1.0 summaries parse host-owned origin and optional presentation without using pack names", () => {
  const cover = `kohbor-media://localhost/v1/cover/${installId}/${sha}`;
  const [pack] = parsePackSummaries({ schema_version: "1.1.0", records: [{
    ...summaryRecord,
    authority_class: "system",
    publisher_claim: null,
    installed_at: "2026-11-20T12:00:00Z",
    size_bytes: 4096,
    game_id: "dota2",
    presentation: { description: "คำอธิบาย", author: "ผู้จัดทำ", languages: ["th", "en"], cover },
  }] })!;
  assert.equal(pack?.authorityClass, "system");
  assert.equal(pack?.presentation.coverUrl, cover);
  assert.deepEqual(pack?.presentation.languages, ["th", "en"]);
  assert.equal(pack?.installedAt, "2026-11-20T12:00:00Z");
  assert.equal(pack?.sizeBytes, 4096);
  assert.equal(pack?.gameId, "dota2");
  assert.equal(parsePackSummaries({ schema_version: "1.1.0", records: [{ ...summaryRecord, authority_class: "publisher" }] }), null);
});

test("pack details validate install-bound visual URLs and retain event variants", () => {
  const pack = parsePackSummaries({ schema_version: "1.0.0", records: [summaryRecord] })![0]!;
  const detail = parseBrowsePackDetail({
    schema_version: "1.1.0",
    install_id: installId,
    package_id: pack.packageId,
    revision: pack.revision,
    match_active: false,
    events: [{
      event_id: "hero_kill", tier: null, state: "ready", reason: "MEDIA_READY",
      resolved_from_tier: null, selection: null,
      variants: [{ variant_id: "voice-1", duration_ms: 1400, has_visual: true, visual_url: `kohbor-media://localhost/v1/visual/${installId}/${sha}` }],
    }],
  }, pack);
  assert.equal(detail?.bindings.find(binding => binding.event_id === "hero_kill")?.variants[0]?.visualUrl,
    `kohbor-media://localhost/v1/visual/${installId}/${sha}`);
  assert.equal(parseBrowsePackDetail({
    schema_version: "1.1.0", install_id: installId, package_id: pack.packageId, revision: pack.revision,
    match_active: false, events: [{
      event_id: "hero_kill", tier: null, state: "ready", reason: "MEDIA_READY", resolved_from_tier: null,
      selection: null, variants: [{ variant_id: "voice-1", duration_ms: 1400, has_visual: true,
        visual_url: `kohbor-media://localhost/v1/visual/ins_0000000002/${sha}` }],
    }],
  }, pack), null);
});

test("preview lifecycle event parsing validates playback identity and monotonic timing", () => {
  const started = parseBrowseStartedEvent({ playback_id: "play_000001", install_id: installId, variant_id: "voice-1",
    started_monotonic_ms: 1000, duration_ms: 1400, has_visual: true });
  assert.equal(started?.installId, installId);
  assert.equal(started?.durationMs, 1400);
  assert.equal(parseBrowseStartedEvent({ playback_id: "bad id", install_id: installId, variant_id: "voice-1",
    started_monotonic_ms: -1, duration_ms: 1400, has_visual: true }), null);
  assert.equal(parseBrowseEndedEvent({ playback_id: "play_000001", install_id: installId, variant_id: "voice-1", reason: "finished" })?.reason,
    "finished");
  assert.equal(parseBrowseEndedEvent({ playback_id: "play_000001", install_id: installId, variant_id: "voice-1", reason: { bad: true } }), null);
});

test("browse audition rejects overlay previews for a non-active pack before calling the host", async () => {
  const pack = parsePackSummaries({ schema_version: "1.0.0", records: [summaryRecord] })![0]!;
  const calls: unknown[] = [];
  const api = createBrowseApi({
    summaries: async () => ({ schema_version: "1.0.0", records: [summaryRecord] }),
    detail: async () => null,
    visualUrl: async () => null,
    audition: async request => { calls.push(request); return null; },
    subscribeStarted: async () => () => {},
    subscribeEnded: async () => () => {},
  });
  await assert.rejects(api.audition({ pack, eventId: "hero_kill", tier: null, variantId: "voice-1", overlay: true }), /OVERLAY_ONLY_ACTIVE_PACK/);
  assert.deepEqual(calls, []);
});

test("source filters do not guess unknown origin and search/sort remain available", () => {
  const [unknown] = parsePackSummaries({ schema_version: "1.0.0", records: [summaryRecord] })!;
  const packs: BrowsePackSummary[] = [
    { ...unknown!, installId: "ins_0000000001", name: "Zeta", active: true },
    { ...unknown!, installId: "ins_0000000002", name: "Alpha", authorityClass: "system", active: false },
    { ...unknown!, installId: "ins_0000000003", name: "Custom", authorityClass: "local_import", active: false },
  ];
  assert.deepEqual(countPackOrigins(packs), { official: 1, custom: 1, unknown: 1 });
  assert.deepEqual(filterBrowsePacks(packs, { official: false, custom: true, query: "", sort: "name" })
    .map(pack => pack.name), ["Alpha", "Custom", "Zeta"]);
  assert.deepEqual(filterBrowsePacks(packs.slice(1), { official: false, custom: true, query: "", sort: "active-first" })
    .map(pack => pack.name), ["Custom"]);
  assert.deepEqual(filterBrowsePacks(packs, { official: true, custom: true, query: "zet", sort: "name" })
    .map(pack => pack.name), ["Zeta"]);
});

test("preview state follows matching started/ended IDs and clears on navigation", () => {
  const initial = createBrowsePreviewState();
  const playing = reduceBrowsePreview(initial, { type: "started", playbackId: "p1", installId,
    variantId: "voice-1", startedMonotonicMs: 10_000, receivedAtMs: 240, durationMs: 1400,
    hasVisual: true, visualUrl: `kohbor-media://localhost/v1/visual/${installId}/${sha}` });
  assert.equal(playing.playbackId, "p1");
  assert.equal(playing.visualUrl, `kohbor-media://localhost/v1/visual/${installId}/${sha}`);
  assert.equal(reduceBrowsePreview(playing, { type: "ended", playbackId: "old", reason: "finished" }), playing);
  assert.equal(reduceBrowsePreview(playing, { type: "reset" }).playbackId, null);
  assert.equal(reduceBrowsePreview(playing, { type: "ended", playbackId: "p1", reason: "finished" }).playbackId, null);
});

test("the game selector reports live Dota state and keeps unsupported choices focusable", () => {
  const items = gameMenuItems("receiving");
  assert.deepEqual(items.map(item => item.id), ["dota2", "league-of-legends", "overwatch", "valorant", "apex-legends", "counter-strike-2"]);
  assert.equal(items[0]?.status, "connected");
  assert.equal(items[0]?.selectable, true);
  assert.ok(items.slice(1).every(item => !item.selectable && item.focusable));
  assert.equal(items[0]?.initials, "D2");
  assert.match(unsupportedGameExplanation("VALORANT", "th"), /ยังไม่รองรับ VALORANT/);
});

test("glass preference defaults to medium, stores only known levels, and recognizes solid fallbacks", () => {
  const values = new Map<string, string>();
  const storage = { getItem: (key: string) => values.get(key) ?? null, setItem: (key: string, value: string) => { values.set(key, value); } };
  assert.equal(DEFAULT_GLASS_LEVEL, "medium");
  assert.equal(readGlassLevel(storage), "medium");
  writeGlassLevel(storage, "high");
  assert.equal(readGlassLevel(storage), "high");
  values.set("kohbor.glass-level", "invalid");
  assert.equal(readGlassLevel(storage), "medium");
  assert.equal(solidGlassFallback("off", false, false), true);
  assert.equal(solidGlassFallback("medium", true, false), true);
  assert.equal(solidGlassFallback("medium", false, true), true);
  assert.equal(solidGlassFallback("medium", false, false), false);
});
