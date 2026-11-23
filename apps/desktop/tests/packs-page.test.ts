import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { test } from "node:test";
import { cardView, nextPacksTab, packBindingView, parsePackSummaries } from "../src/packs/model.ts";
import { auditionSound, groupSoundRows } from "../src/packs/eventMap.ts";
import { soundRowPresentation } from "../src/packs/sound-row-model.ts";

const packsPageSource = readFileSync(new URL("../src/packs/PacksPage.tsx", import.meta.url), "utf8");
const packLibrarySource = readFileSync(new URL("../src/packs/PackLibrary.tsx", import.meta.url), "utf8");
const soundRowsSource = readFileSync(new URL("../src/packs/SoundRows.tsx", import.meta.url), "utf8");
const variantRowSource = readFileSync(new URL("../src/packs/VariantRow.tsx", import.meta.url), "utf8");

test("installed-packs primary action opens InstallDialog and refreshes the cards after install", () => {
  assert.match(packsPageSource, /className="primary packs-install-trigger"[^>]*aria-haspopup="dialog"[^>]*onClick=\{\(\) => setInstallDialogOpen\(true\)\}/s);
  assert.match(packsPageSource, /installDialogOpen && <InstallDialog/);
  assert.match(packsPageSource, /onInstalled=\{refresh\}/);
});

test("pack details and expandable event-sounds rows wire their audition buttons", () => {
  assert.match(packLibrarySource, /<ul className="packs-voice-list">[\s\S]*?onAuditionVariant\(binding, variant\.variant_id\)/);
  assert.match(soundRowsSource, /const key = soundKey\(row\);[\s\S]*?aria-expanded=\{expanded === key\}/);
  assert.match(soundRowsSource, /<EventVariants variants=\{row\.variants\} selection=\{row\.selection\}/);
  assert.match(packsPageSource, /mediaAuditionVariant\(row\.event_id, row\.tier, variantId\)/);
});

test("unavailable event rows without voices render no primary play control", () => {
  for (const state of ["unmapped", "detector_unavailable", "not_voiced"] as const) {
    assert.equal(soundRowPresentation(state, 0).showPrimaryPlay, false, state);
  }
  assert.equal(soundRowPresentation("detector_unavailable", 1).showPrimaryPlay, true,
    "a pack voice stays auditionable even when the detector cannot fire it yet");
  for (const state of ["ready", "inherited", "invalid_media", "unknown"] as const) {
    assert.equal(soundRowPresentation(state, 1).disablePrimaryPlay, false, `${state}: ListenControl.block decides`);
  }
  assert.match(soundRowsSource, /presentation\.showPrimaryPlay && <ListenButton/);
});

test("a ready event row has one play control with its listenFor accessible name", () => {
  assert.equal(soundRowPresentation("ready", 2).showPrimaryPlay, true);
  assert.equal((soundRowsSource.match(/<ListenButton\b/g) ?? []).length, 1);
  assert.match(soundRowsSource, /aria-label=\{text\.listenFor\(label\)\}/);
});

test("sound expansion retains its expanded state and controlled panel", () => {
  assert.match(soundRowsSource, /aria-expanded=\{expanded === key\}[^>]*aria-controls=\{variantsId\}/);
  assert.match(soundRowsSource, /id=\{variantsId\} className="packs-variants-expanded"/);
});

test("variant and clip ids stay out of visible voice text", () => {
  assert.match(variantRowSource, /data-variant-id=\{variant\.variant_id\}/);
  assert.match(variantRowSource, /data-clip-id=\{variant\.clip_id/);
  assert.doesNotMatch(variantRowSource, />\s*\{variant\.variant_id\}\s*</);
  assert.doesNotMatch(variantRowSource, />\s*\{variant\.clip_id\}\s*</);
});

test("event-sound rows do not render readiness chips", () => {
  assert.doesNotMatch(soundRowsSource, /\bChip\b|chip-glyph/);
});

const install = "ins_0000000001";
const row = {
  install_id: install, package_id: "voice.private-id", revision: 2,
  active: true, pending: false, display_name: "เสียงสนาม", problem: null,
  cover_url: null, event_count: 1, binding_count: 2, variant_count: 3, banner_count: 0,
  bindings: [
    { event_id: "multi_kill", tier: 2, selection: "shuffle", variant_count: 2, banner_count: 0 },
    { event_id: "multi_kill", tier: 3, selection: null, variant_count: 1, banner_count: 0 },
  ],
};

test("a pack card shows real counts, status, revision and an initial without inventing a description", () => {
  const [pack] = parsePackSummaries({ schema_version: "1.0.0", records: [row] })!;
  const card = cardView(pack!, "th");
  assert.equal(card.name, "เสียงสนาม");
  assert.equal(card.initial, "เ");
  assert.equal(card.status, "active");
  assert.equal(card.summary, "พากย์ 1 เหตุการณ์ · 3 เสียง · มีแบนเนอร์ 0");
  assert.equal(card.revision, "รุ่น r2");
  assert.equal(card.coverUrl, null);
  assert.equal(Object.hasOwn(card, "description"), false);
  assert.equal(cardView({ ...pack!, active: false, pending: true }, "th").status, "installed");
  const detail = JSON.stringify(pack!.bindings.map(binding => packBindingView(binding, "th")));
  assert.doesNotMatch(detail, /voice\.private-id|multi_kill|package_id|sha256|schema_version/);
  assert.match(detail, /Double Kill/);
  assert.match(detail, /ระดับ 2/);
  assert.match(detail, /สุ่ม/);
});

test("an invalid cover is reported without exposing another pack's visual", () => {
  const cover = "kohbor-media://localhost/v1/cover/ins_0000000002/" + "a".repeat(64);
  const good = cover.replace("0000000002", "0000000001");
  const visual = { ...row, banner_count: 1, bindings: [{ ...row.bindings[0], banner_count: 1 }, row.bindings[1]] };
  const [pack] = parsePackSummaries({ schema_version: "1.0.0", records: [{ ...visual, problem: "COVER_INVALID", cover_url: null }] })!;
  assert.equal(cardView(pack!, "th").status, "problem");
  assert.equal(cardView(pack!, "th").coverUrl, null);
  const [verified] = parsePackSummaries({ schema_version: "1.0.0", records: [{ ...visual, cover_url: good }] })!;
  assert.equal(cardView(verified!, "th").coverUrl, good);
});

test("a pack with banners but no presentation cover shows its initial and no cover problem", () => {
  const visual = { ...row, banner_count: 2, bindings: [{ ...row.bindings[0], banner_count: 2 }, row.bindings[1]] };
  const modern = { authority_class: "local_import", publisher_claim: null, installed_at: 1795176000000, size_bytes: 4096, game_id: "dota2", presentation: null };
  for (const schema_version of ["1.0.0", "1.1.0"] as const) {
    const record = { ...visual, problem: null, cover_url: null, ...(schema_version === "1.1.0" ? modern : {}) };
    const [pack] = parsePackSummaries({ schema_version, records: [record] })!;
    assert.equal(pack!.problem, null, schema_version);
    assert.equal(pack!.coverUrl, null, schema_version);
    const card = cardView(pack!, "th");
    assert.equal(card.status, "active", schema_version);
    assert.equal(card.reason, null, schema_version);
    assert.equal(card.initial, "เ", schema_version);
  }
});

test("a pack without banners keeps its valid presentation cover", () => {
  const cover = "kohbor-media://localhost/v1/cover/ins_0000000001/" + "b".repeat(64);
  const [pack] = parsePackSummaries({ schema_version: "1.0.0", records: [{ ...row, cover_url: cover }] })!;
  assert.equal(pack!.counts?.banners, 0);
  assert.equal(pack!.problem, null);
  assert.equal(cardView(pack!, "th").coverUrl, cover);
});

test("the card cover is a still image, not a video", () => {
  const cover = packLibrarySource.slice(packLibrarySource.indexOf("function PackCover"), packLibrarySource.indexOf("function categoryCoverage"));
  assert.match(cover, /<img className=\{ready \? "is-ready" : undefined\} src=\{src\}/);
  assert.match(cover, /onError=\{\(\) => \{ setFailed\(true\); reportFailure\(\); \}\}/);
  assert.doesNotMatch(cover, /<video|currentTime|onSeeked|onLoadedMetadata/);
  assert.match(packLibrarySource, /<img className="packs-preview-poster" src=\{packCoverWebviewUrl\(pack\.coverUrl, PLATFORM\)/);
});

test("the CSP img-src admits the media host for pack covers and nothing else changes scope", () => {
  const conf = JSON.parse(readFileSync(new URL("../src-tauri/tauri.conf.json", import.meta.url), "utf8"));
  const csp: string = conf.app.security.csp;
  const directives = new Map(csp.split(";").map(part => part.trim().split(/\s+/)).map(([name, ...values]) => [name!, values]));
  assert.deepEqual(directives.get("img-src"), ["'self'", "data:", "http://kohbor-media.localhost", "kohbor-media:"]);
  assert.deepEqual(directives.get("media-src"), ["http://kohbor-media.localhost", "kohbor-media:"]);
  assert.deepEqual(directives.get("default-src"), ["'self'"]);
  assert.deepEqual(directives.get("connect-src"), ["ipc:", "http://ipc.localhost"]);
});

test("tabs open on installed packs and navigation wraps or jumps to the ends", () => {
  assert.equal(nextPacksTab(undefined, "Home"), "installed");
  assert.equal(nextPacksTab("installed", "ArrowRight"), "events");
  assert.equal(nextPacksTab("installed", "ArrowLeft"), "recap");
  assert.equal(nextPacksTab("events", "End"), "recap");
  assert.equal(nextPacksTab("recap", "ArrowRight"), "installed");
  assert.equal(nextPacksTab("recap", "Tab"), null);
});

test("game groups keep each event and tier, including just one unknown row", () => {
  const [groups] = [groupSoundRows([
    { event_id: "multi_kill", tier: 3, state: "unknown" },
    { event_id: "hero_kill", tier: null, state: "ready" },
    { event_id: "multi_kill", tier: 2, state: "ready" },
  ])];
  assert.deepEqual(groups.map(group => [group.id, group.rows.map(row => row.state)]),
    [["kill", ["ready"]], ["multi", ["ready", "unknown"]]]);
});

test("Listen forwards the event and tier once, and does not call a blocked audition", async () => {
  const calls: [string, number | null][] = [];
  const command = async (event: string, tier: number | null) => { calls.push([event, tier]); return true; };
  const sound = { event_id: "multi_kill", tier: 3 };
  assert.equal(await auditionSound(command, sound, "AUDITION_MATCH_ACTIVE"), null);
  assert.equal(await auditionSound(command, sound, null), true);
  assert.deepEqual(calls, [["multi_kill", 3]]);
});

test("the last-match tab uses the stored history and connects the host read and clear", () => {
  assert.match(packsPageSource, /from "\.\/MatchHistory"/);
  assert.match(packsPageSource, /name === "recap"[\s\S]+<MatchHistory/);
  assert.match(packsPageSource, /loadMatchRecap\(matchRecapCommand\)/);
  assert.match(packsPageSource, /await clearMatchRecap\(matchRecapCommand\)/);
  assert.doesNotMatch(packsPageSource, /<MatchRecap/);
});
