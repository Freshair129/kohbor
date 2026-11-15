import assert from "node:assert/strict";
import test from "node:test";
import {
  bindingLabel,
  bindingsFor,
  clipChoices,
  draftProblem,
  parseActiveBindings,
  parseDraft,
  playAvailability,
  toMappingDocument,
  type DraftBinding,
  type MappableEvent,
} from "../src/lab/labMapping.ts";

const ACTIVE = {
  active: { install_id: "abcdefgh12", package_id: "dev.pack", revision: 3 },
  bindings: [
    { event_id: "hero_kill", tier: null, audio_name: "hero_kill.wav", duration_ms: 1500, has_visual: true, state: "ok" },
    { event_id: "streak", tier: 4, audio_name: "streak-t4.wav", duration_ms: 2000, has_visual: false, state: "ok" },
    { event_id: "streak", tier: 3, audio_name: "streak-t3.wav", duration_ms: 1800, has_visual: false, state: "invalid" },
  ],
};

const EVENTS: MappableEvent[] = [
  { id: "hero_kill", tierRange: null },
  { id: "multi_kill", tierRange: [2, 5] },
  { id: "streak", tierRange: [3, 10] },
];

test("the host listing parses; unbound events simply have no rows", () => {
  const parsed = parseActiveBindings(ACTIVE);
  assert.ok(parsed);
  assert.equal(parsed.active?.packageId, "dev.pack");
  assert.equal(parsed.bindings.length, 3);
  assert.deepEqual(bindingsFor("streak", parsed.bindings).map((b) => b.tier), [3, 4], "sorted by tier");
  assert.deepEqual(bindingsFor("hero_death", parsed.bindings), [], "unmapped");
  assert.deepEqual(parseActiveBindings({ active: null, bindings: [] }), { active: null, bindings: [] });
  assert.equal(parseActiveBindings({ active: null, bindings: [{ event_id: 1 }] }), null);
  assert.equal(parseActiveBindings({ bindings: [{ event_id: "a", audio_name: "a.wav", duration_ms: 1, state: "maybe" }] }), null);
  assert.equal(parseActiveBindings(null), null);
});

test("labels show file, duration, banner and a failed re-check", () => {
  const parsed = parseActiveBindings(ACTIVE);
  assert.ok(parsed);
  assert.equal(bindingLabel(parsed.bindings[0]), "hero_kill.wav · 1.5 วิ · มีแบนเนอร์");
  assert.equal(bindingLabel(parsed.bindings[1]), "tier 4: streak-t4.wav · 2.0 วิ");
  assert.match(bindingLabel(parsed.bindings[2]), /ตรวจซ้ำไม่ผ่าน/);
});

test("play needs a verified clip and the demo trigger, and says why not", () => {
  const parsed = parseActiveBindings(ACTIVE);
  assert.ok(parsed);
  assert.deepEqual(playAvailability(parsed.bindings[0], true), { ok: true });
  const noDemo = playAvailability(parsed.bindings[0], false);
  assert.equal(noDemo.ok, false);
  const invalid = playAvailability(parsed.bindings[2], true);
  assert.equal(invalid.ok, false);
});

test("clip choices are the active pack's verified clips, once each", () => {
  const parsed = parseActiveBindings(ACTIVE);
  assert.ok(parsed);
  assert.deepEqual(clipChoices(parsed.bindings), ["hero_kill.wav", "streak-t4.wav"]);
  assert.deepEqual(clipChoices([]), []);
});

test("the draft form rejects impossible entries before the host does", () => {
  const draft: DraftBinding[] = [{ eventId: "hero_kill", tier: null, audio: "a.wav" }];
  const problem = (entry: DraftBinding) => draftProblem(entry, EVENTS, draft);
  assert.equal(problem({ eventId: "hero_death", tier: null, audio: "a.wav" }), "unknown_event");
  assert.equal(problem({ eventId: "hero_kill", tier: null, audio: "b.wav" }), "duplicate");
  assert.equal(problem({ eventId: "hero_kill", tier: 2, audio: "b.wav" }), "tier_not_allowed");
  assert.equal(problem({ eventId: "multi_kill", tier: null, audio: "b.wav" }), "tier_required");
  assert.equal(problem({ eventId: "multi_kill", tier: 6, audio: "b.wav" }), "tier_out_of_range");
  assert.equal(problem({ eventId: "multi_kill", tier: 2, audio: "../b.wav" }), "bad_audio");
  assert.equal(problem({ eventId: "multi_kill", tier: 2, audio: "dir/b.wav" }), "bad_audio");
  assert.equal(problem({ eventId: "multi_kill", tier: 2, audio: "b.mp3" }), "bad_audio");
  assert.equal(problem({ eventId: "multi_kill", tier: 2, audio: "b.wav" }), null);
  assert.equal(problem({ eventId: "multi_kill", tier: 3, audio: "b.wav" }), null, "another tier of a bound event is fine");
});

test("the wire document is the rebuild-tool format and tiers appear only where they exist", () => {
  const doc = toMappingDocument([
    { eventId: "hero_kill", tier: null, audio: "06_eliminate-enemy.wav" },
    { eventId: "streak", tier: 3, audio: "x.wav" },
  ]);
  assert.deepEqual(doc, {
    schema_version: "1.0.0",
    bindings: [
      { event_id: "hero_kill", audio: "06_eliminate-enemy.wav" },
      { event_id: "streak", tier: 3, audio: "x.wav" },
    ],
  });
  assert.deepEqual(toMappingDocument([]), { schema_version: "1.0.0", bindings: [] });
});

test("a saved draft parses back, nothing saved is an empty draft, junk is unreadable", () => {
  assert.deepEqual(parseDraft(null), []);
  assert.deepEqual(parseDraft({ bindings: [{ event_id: "streak", tier: 3, audio: "x.wav" }] }), [
    { eventId: "streak", tier: 3, audio: "x.wav" },
  ]);
  assert.equal(parseDraft({ bindings: [{ event_id: 1 }] }), null);
  assert.equal(parseDraft("x"), null);
});
