import assert from "node:assert/strict";
import { test } from "node:test";

const history = await import("../src/history.ts");

const admitted = {
  schema_version: "1.0.0",
  instance_id: "ev_g3_s10_0",
  event_type: "hero_kill",
  session_epoch: 2,
  audio_class: "ordinary",
  outcome: "admitted",
  playback_id: "pb_0000000001",
  package_revision: 4,
  decided_monotonic_ms: 61_500,
};
const unmapped = { ...admitted, instance_id: "ev_g3_s11_0", event_type: "health_danger", audio_class: "critical", outcome: "unmapped", playback_id: null, package_revision: null };

test("parses a valid outcome record", () => {
  assert.deepEqual(history.parseOutcomeRecord(admitted), {
    instance_id: "ev_g3_s10_0",
    event_type: "hero_kill",
    session_epoch: 2,
    audio_class: "ordinary",
    outcome: "admitted",
    playback_id: "pb_0000000001",
    package_revision: 4,
    decided_monotonic_ms: 61_500,
  });
  assert.equal(history.parseOutcomeRecord(unmapped)?.outcome, "unmapped");
});

test("rejects records that break the closed shape or the conditional rules", () => {
  assert.equal(history.parseOutcomeRecord({ ...admitted, extra: 1 }), null);
  assert.equal(history.parseOutcomeRecord({ ...admitted, schema_version: "2.0.0" }), null);
  assert.equal(history.parseOutcomeRecord({ ...admitted, session_epoch: 0 }), null);
  assert.equal(history.parseOutcomeRecord({ ...admitted, outcome: "heard" }), null);
  assert.equal(history.parseOutcomeRecord({ ...admitted, audio_class: "loud" }), null);
  assert.equal(history.parseOutcomeRecord({ ...admitted, decided_monotonic_ms: -1 }), null);
  assert.equal(history.parseOutcomeRecord({ ...admitted, event_type: "Hero Kill" }), null);
  // admitted/preempted need a playback id; every other outcome must not carry one
  assert.equal(history.parseOutcomeRecord({ ...admitted, playback_id: null }), null);
  assert.equal(history.parseOutcomeRecord({ ...unmapped, playback_id: "pb_0000000002" }), null);
  // a non-voice admission carries its package revision
  assert.equal(history.parseOutcomeRecord({ ...admitted, package_revision: null }), null);
  // voice records have no event type or revision; other classes must have an event type
  assert.equal(history.parseOutcomeRecord({ ...admitted, audio_class: "voice" }), null);
  assert.equal(history.parseOutcomeRecord({ ...admitted, audio_class: "voice", event_type: null, package_revision: null })?.audio_class, "voice");
  assert.equal(history.parseOutcomeRecord({ ...unmapped, event_type: null }), null);
  assert.equal(history.parseOutcomeRecord(null), null);
  assert.equal(history.parseOutcomeRecord([]), null);
});

test("history is newest first and counts skipped records instead of guessing", () => {
  const parsed = history.parseAudioHistory([admitted, { bad: true }, unmapped]);
  assert.ok(parsed);
  assert.equal(parsed.skipped, 1);
  assert.deepEqual(parsed.records.map((record) => record.instance_id), ["ev_g3_s11_0", "ev_g3_s10_0"]);
  assert.deepEqual(history.parseAudioHistory([]), { records: [], skipped: 0 });
  assert.equal(history.parseAudioHistory({ records: [] }), null);
  assert.equal(history.parseAudioHistory(null), null);
});

test("every contract outcome has a tone and only admission reads as played", () => {
  for (const outcome of history.HISTORY_OUTCOMES) {
    assert.ok(["played", "blocked", "silent"].includes(history.outcomeTone(outcome)));
  }
  assert.equal(history.outcomeTone("admitted"), "played");
  assert.equal(history.outcomeTone("muted"), "blocked");
  assert.equal(history.outcomeTone("unmapped"), "silent");
  assert.equal(history.HISTORY_OUTCOMES.filter((o) => history.outcomeTone(o) === "played").length, 1);
});

test("formats elapsed host time", () => {
  assert.equal(history.formatElapsed(0), "0:00");
  assert.equal(history.formatElapsed(61_500), "1:01");
  assert.equal(history.formatElapsed(3_723_000), "1:02:03");
});

test("recognizes the reserved DEMO instance generation without extending the outcome schema", () => {
  const demoRecord = { ...unmapped, instance_id: "ev_g18446744073709551615_s1_0" };
  assert.ok(history.parseOutcomeRecord(demoRecord));
  assert.equal(history.isDemoOutcomeRecord(demoRecord), true);
  assert.equal(history.isDemoOutcomeRecord(admitted), false);
  assert.equal(history.parseOutcomeRecord({ ...demoRecord, source: "demo" }), null);
});
