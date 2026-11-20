import assert from "node:assert/strict";
import { test } from "node:test";
import { cardView, nextPacksTab, packBindingView, parsePackSummaries, sessionAudio } from "../src/packs/model.ts";
import { auditionSound, groupSoundRows } from "../src/packs/eventMap.ts";

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
  const [pack] = parsePackSummaries({ schema_version: "1.0.0", records: [{ ...visual, cover_url: cover }] })!;
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

test("the session recap hides auditions, demo and voice requests and combines preempted playback", () => {
  const event = { instance_id: "ev_g1_s1_0", event_type: "hero_kill", session_epoch: 1,
    audio_class: "ordinary", playback_id: "player0001", package_revision: 1, decided_monotonic_ms: 50, outcome: "admitted" } as const;
  const records = sessionAudio({ skipped: 0, records: [
    { ...event, event_type: "audition" }, { ...event, event_type: null, audio_class: "voice" },
    { ...event, instance_id: "ev_g18446744073709551615_s1_0" },
    { ...event, outcome: "preempted", decided_monotonic_ms: 80 }, event,
    { ...event, instance_id: "ev_g1_s2_0", event_type: "hero_death", outcome: "unmapped" },
  ] });
  assert.deepEqual(records.map(item => item.outcome), ["preempted", "unmapped"]);
});
