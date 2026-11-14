import assert from "node:assert/strict";
import { test } from "node:test";
import { deriveEventFeed, deriveLastClip, deriveOnAir, relativeHistoryAge } from "../src/deck/eventFeedModel.ts";
import { createPlaybackState, reducePlayback, tickPlayback } from "../src/playback.ts";
import { readAudioHistory } from "../src/history.ts";
import { hostNowFromAnchor } from "../src/hostClock.ts";
import type { AudioOutcomeRecord, HistoryOutcome } from "../src/history.ts";

const record = (at = 1000, outcome: HistoryOutcome = "admitted"): AudioOutcomeRecord => ({
  instance_id: `ev_g1_s${at}_0`, event_type: "health_low", session_epoch: 1, audio_class: "ordinary",
  outcome, playback_id: outcome === "admitted" || outcome === "preempted" ? `pb_${at.toString().padStart(8, "0")}` : null,
  package_revision: 1, decided_monotonic_ms: at,
});
const history = (records: AudioOutcomeRecord[]) => ({ records, skipped: 0 });
const started = (overrides = {}) => ({ kind: "started", playback_id: "pb_00000001", class: "ordinary", event_type: "health_low", tier: null, visual_url: null, audio_duration_ms: 1000, visual_tail_ms: 0, at_ms: 1000, ...overrides });

test("feed orders real records newest first and bounds the scrolling list to 30", () => {
  const records = Array.from({ length: 40 }, (_, n) => record(n + 1));
  const feed = deriveEventFeed(history(records), "ok", 5000, "th");
  assert.equal(feed.rows.length, 30);
  assert.equal(feed.rows[0]?.record.decided_monotonic_ms, 40);
  assert.equal(feed.rows[29]?.record.decided_monotonic_ms, 11);
  assert.equal(records[0]?.decided_monotonic_ms, 1, "source is not reordered");
});

test("outcome words, catalog labels, class and DEMO are derived without clip names", () => {
  for (const [outcome, word] of Object.entries({ admitted: "เล่นเสียง", muted: "ปิดเสียง", unmapped: "ไม่มีคลิป", cooldown: "พักก่อน", preempted: "ถูกแทรก", not_voiced: "ไม่มีเสียงประกาศ", channel_busy: "ไม่เล่น" })) {
    const event = record(1000, outcome as HistoryOutcome);
    event.instance_id = "ev_g18446744073709551615_s1_0";
    const row = deriveEventFeed(history([event]), "ok", 4500, "th").rows[0]!;
    assert.equal(row.outcomeLabel, word); assert.equal(row.eventLabel, "เลือดต่ำ");
    assert.equal(row.audioClass, "ordinary"); assert.equal(row.demo, true);
    assert.equal(row.age, "3 วิ"); assert.equal(row.outcomeCode, outcome);
  }
  const unknown = record(); unknown.event_type = "future_event";
  assert.equal(deriveEventFeed(history([unknown]), "ok", null, "th").rows[0]?.eventLabel, "future_event");
});

test("empty, loading and failed histories never fabricate rows", () => {
  assert.equal(deriveEventFeed(history([]), "ok", null, "th").message, "ยังไม่มีเหตุการณ์");
  for (const state of ["failed", "unreadable"] as const) {
    const feed = deriveEventFeed(history([record()]), state, null, "th");
    assert.equal(feed.message, "อ่านเหตุการณ์ไม่ได้"); assert.deepEqual(feed.rows, []);
  }
  assert.deepEqual(deriveEventFeed(null, "loading", null, "th").rows, []);
  for (const now of [null, 999, Number.NaN]) assert.equal(relativeHistoryAge(1000, now, "th"), "—");
});

test("last clip is the newest admitted playback, ignoring newer blocked or id-less records", () => {
  const noId = { ...record(5000), playback_id: null };
  const last = deriveLastClip(history([record(1000), record(4000, "muted"), record(3000), noId]), "ok", 6000, "th");
  assert.equal(last?.record.decided_monotonic_ms, 3000); assert.equal(last?.age, "3 วิ");
  assert.equal(deriveLastClip(history([record(4000, "muted")]), "ok", null, "th"), null);
  assert.equal(deriveLastClip(history([record()]), "failed", 6000, "th"), null);
});

test("ON AIR follows audio-only, voice and visual lifecycle, never the retained duplicate fence", () => {
  let state = reducePlayback(createPlaybackState(), started());
  assert.equal(deriveOnAir(state, "th")?.eventLabel, "เลือดต่ำ");
  state = reducePlayback(state, { kind: "ended", playback_id: "pb_00000001", reason: "finished", at_ms: 2000 });
  assert.equal(state.currentPlaybackId, "pb_00000001"); assert.equal(deriveOnAir(state, "th"), null);
  const voice = reducePlayback(createPlaybackState(), started({ class: "voice", event_type: null }));
  assert.ok(deriveOnAir(voice, "th"));
  assert.equal(deriveOnAir(tickPlayback(voice, 2000), "th"), null, "missing end cannot leave an audio-only ON AIR stuck");
  const visual = reducePlayback(createPlaybackState(), started({ visual_url: "kohbor-media://localhost/clip", visual_tail_ms: 500 }));
  assert.ok(deriveOnAir(tickPlayback(visual, 2000), "th"), "visual tail is still active");
  assert.equal(deriveOnAir(tickPlayback(visual, 3000), "th"), null);
});

test("shared history reader fetches the host's full bounded buffer before picking newest rows", async () => {
  const raw = Array.from({ length: 150 }, (_, n) => ({ schema_version: "1.0.0", ...record(n + 1) }));
  const parsed = await readAudioHistory(async limit => raw.slice(0, limit));
  assert.equal(deriveEventFeed(parsed, "ok", null, "th").rows[0]?.record.decided_monotonic_ms, 150);
});

test("ages require an observed host clock anchor and use elapsed monotonic time", () => {
  assert.equal(hostNowFromAnchor(null, 90000), null);
  assert.equal(hostNowFromAnchor({ hostMs: 5000, localMs: 90000 }, 93000), 8000);
  assert.equal(hostNowFromAnchor({ hostMs: 5000, localMs: 90000 }, 89999), null);
  assert.equal(hostNowFromAnchor({ hostMs: Number.NaN, localMs: 90000 }, 93000), null);
  assert.equal(relativeHistoryAge(5000, hostNowFromAnchor({ hostMs: 5000, localMs: 90000 }, 93000), "th"), "3 วิ");
});

test("shared reader propagates command errors and never turns malformed data into an empty history", async () => {
  await assert.rejects(readAudioHistory(async () => { throw new Error("HOST_OFFLINE"); }), /HOST_OFFLINE/);
  assert.equal(await readAudioHistory(async () => ({ records: [] })), null);
});

test("old end events and duplicate starts cannot clear or revive the current audio", () => {
  let state = reducePlayback(createPlaybackState(), started());
  state = reducePlayback(state, started({ playback_id: "pb_00000002", event_type: "hero_death", at_ms: 1500 }));
  state = reducePlayback(state, { kind: "ended", playback_id: "pb_00000001", reason: "finished", at_ms: 2000 });
  assert.equal(deriveOnAir(state, "th")?.playbackId, "pb_00000002");
  state = reducePlayback(state, { kind: "ended", playback_id: "pb_00000002", reason: "finished", at_ms: 2500 });
  state = reducePlayback(state, started({ playback_id: "pb_00000002", at_ms: 2600 }));
  assert.equal(deriveOnAir(state, "th"), null);
});
