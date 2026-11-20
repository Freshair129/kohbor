import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { test } from "node:test";
import { cardView, nextPacksTab, packBindingView, parsePackSummaries } from "../src/packs/model.ts";
import { auditionSound, groupSoundRows } from "../src/packs/eventMap.ts";

const packsPageSource = readFileSync(new URL("../src/packs/PacksPage.tsx", import.meta.url), "utf8");
const packLibrarySource = readFileSync(new URL("../src/packs/PackLibrary.tsx", import.meta.url), "utf8");
const soundRowsSource = readFileSync(new URL("../src/packs/SoundRows.tsx", import.meta.url), "utf8");

test("installed-packs primary action opens InstallDialog and refreshes the cards after install", () => {
  assert.match(packsPageSource, /className="primary packs-install-trigger"[^>]*aria-haspopup="dialog"[^>]*onClick=\{\(\) => setInstallDialogOpen\(true\)\}/s);
  assert.match(packsPageSource, /installDialogOpen && <InstallDialog/);
  assert.match(packsPageSource, /onInstalled=\{refresh\}/);
});

test("event variants mount in the pack detail and expandable event-sounds rows", () => {
  assert.match(packLibrarySource, /<EventVariants variants=\{binding\.variants\} selection=\{binding\.selection\}/);
  assert.match(packLibrarySource, /onAudition=\{variantId => onAuditionVariant\(binding, variantId\)\}/);
  assert.match(soundRowsSource, /aria-expanded=\{expanded === soundKey\(row\)\}/);
  assert.match(soundRowsSource, /<EventVariants variants=\{row\.variants\} selection=\{row\.selection\}/);
  assert.match(packsPageSource, /mediaAuditionVariant\(row\.event_id, row\.tier, variantId\)/);
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
