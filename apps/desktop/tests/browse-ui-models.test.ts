import assert from "node:assert/strict";
import { test } from "node:test";
import { createBrowseApi, createBrowseAuditionPort, type BrowsePackSummary } from "../src/packs/browseApi.ts";
import { parsePackSummaries } from "../src/packs/model.ts";
import { filterBrowsePacks, countPackOrigins } from "../src/packs/browseModel.ts";
import { createBrowsePreviewState, reduceBrowsePreview } from "../src/packs/previewModel.ts";
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

test("1.0.0 summaries keep authority unknown and reject fields outside the host schema", () => {
  const [pack] = parsePackSummaries({ schema_version: "1.0.0", records: [summaryRecord] })!;
  assert.equal(pack?.authorityClass, "unknown");
  assert.equal(pack?.presentation.description, null);
  assert.equal(pack?.publisherClaim, null);
  assert.equal(parsePackSummaries({ schema_version: "1.0.0", records: [{ ...summaryRecord, authority_class: "system" }] }), null);
});

test("browse adapter keeps 1.0.0 authority explicitly unknown until the host supplies 1.1.0", async () => {
  const api = createBrowseApi({
    summaries: async () => ({ schema_version: "1.0.0", records: [summaryRecord] }),
    detail: async () => null, visualUrl: async () => null, audition: async () => null,
    subscribe: async () => () => {},
  });
  const [pack] = await api.summaries() ?? [];
  assert.equal(pack?.authorityClass, "unknown");
});

test("1.1.0 summaries parse host-owned origin and optional presentation without using pack names", () => {
  const cover = `kohbor-media://localhost/v1/cover/${installId}/${sha}`;
  const [pack] = parsePackSummaries({ schema_version: "1.1.0", records: [{
    ...summaryRecord,
    cover_url: cover,
    authority_class: "system",
    publisher_claim: null,
    installed_at: 1795176000000,
    size_bytes: 4096,
    game_id: "dota2",
    presentation: { description: "คำอธิบาย", author: "ผู้จัดทำ", languages: ["th", "en"], cover_url: cover },
  }] })!;
  assert.equal(pack?.authorityClass, "system");
  assert.equal(pack?.presentation.coverUrl, cover);
  assert.deepEqual(pack?.presentation.languages, ["th", "en"]);
  assert.equal(pack?.installedAt, "2026-11-20T12:00:00.000Z");
  assert.equal(pack?.sizeBytes, 4096);
  assert.equal(pack?.gameId, "dota2");
  assert.equal(parsePackSummaries({ schema_version: "1.1.0", records: [{ ...summaryRecord, authority_class: "publisher" }] }), null);
});

test("official and custom browse filters use the host authority_class summary field", () => {
  const base = { ...summaryRecord, event_count: 1, binding_count: 1, variant_count: 1, banner_count: 1 };
  const packs = parsePackSummaries({ schema_version: "1.1.0", records: [
    { ...base, authority_class: "system", publisher_claim: null, installed_at: null, size_bytes: 1, game_id: "dota2", presentation: null },
    { ...base, install_id: "ins_0000000002", package_id: "custom.pack", authority_class: "local_import", publisher_claim: null,
      installed_at: null, size_bytes: 1, game_id: "dota2", presentation: null },
  ] })!;
  assert.deepEqual(filterBrowsePacks(packs, { official: true, custom: false, query: "", sort: "name" }).map(pack => pack.authorityClass), ["system"]);
  assert.deepEqual(filterBrowsePacks(packs, { official: false, custom: true, query: "", sort: "name" }).map(pack => pack.authorityClass), ["local_import"]);
});

test("audition dispatch uses the installed-variant command for non-active packs and active command otherwise", async () => {
  const calls: string[] = [];
  const port = createBrowseAuditionPort(async () => { calls.push("active"); return null; }, async () => { calls.push("installed"); return null; });
  const [inactive] = parsePackSummaries({ schema_version: "1.0.0", records: [summaryRecord] })!;
  await port({ pack: inactive!, eventId: "hero_kill", tier: null, variantId: "voice-1", overlay: false });
  await port({ pack: { ...inactive!, active: true }, eventId: "hero_kill", tier: null, variantId: "voice-1", overlay: true });
  assert.deepEqual(calls, ["installed", "active"]);
});

test("browse audition rejects overlay previews for a non-active pack before calling the host", async () => {
  const pack = parsePackSummaries({ schema_version: "1.0.0", records: [summaryRecord] })![0]!;
  const calls: unknown[] = [];
  const api = createBrowseApi({
    summaries: async () => ({ schema_version: "1.0.0", records: [summaryRecord] }),
    detail: async () => null,
    visualUrl: async () => null,
    audition: async request => { calls.push(request); return null; },
    subscribe: async () => () => {},
  });
  await assert.rejects(api.audition({ pack, eventId: "hero_kill", tier: null, variantId: "voice-1", overlay: true }), /OVERLAY_ONLY_ACTIVE_PACK/);
  assert.deepEqual(calls, []);
});

test("verified visual URL and host audition lifecycle events map into the browse sync model", async () => {
  let emit: ((event: any) => void) | undefined;
  const api = createBrowseApi({
    summaries: async () => ({ schema_version: "1.0.0", records: [summaryRecord] }),
    detail: async () => null,
    visualUrl: async () => ({ visual_url: `kohbor-media://localhost/v1/${installId}/${sha}` }),
    audition: async () => ({ audio_started: true, audio_ended: false, audio_end_reason: null,
      visual_status: "shown", visual_reason: null, video_error_code: null }),
    subscribe: async callback => { emit = callback; return () => {}; },
  });
  assert.equal(await api.visualUrl(installId, "hero_kill", null, "voice-1"), `kohbor-media://localhost/v1/${installId}/${sha}`);
  const [pack] = await api.summaries() ?? [];
  await api.audition({ pack: pack!, eventId: "hero_kill", tier: null, variantId: "voice-1", overlay: false });
  const events: unknown[] = [];
  await api.subscribe(event => events.push(event));
  emit!({ kind: "started", playback_id: "play_000001", class: "voice", event_type: "audition", tier: null,
    visual_url: `kohbor-media://localhost/v1/${installId}/${sha}`, audio_duration_ms: 1400, visual_tail_ms: 0, at_ms: 1000 });
  emit!({ kind: "ended", playback_id: "play_000001", reason: "finished", at_ms: 2400 });
  assert.deepEqual(events, [
    { playbackId: "play_000001", installId, variantId: "voice-1", startedMonotonicMs: 1000, durationMs: 1400,
      hasVisual: true, visualUrl: `kohbor-media://localhost/v1/${installId}/${sha}` },
    { playbackId: "play_000001", installId, variantId: "voice-1", reason: "finished" },
  ]);
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
