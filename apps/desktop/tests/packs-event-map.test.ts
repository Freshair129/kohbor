import assert from "node:assert/strict";
import { test } from "node:test";
import catalog from "../../../contracts/event-catalog.json" with { type: "json" };

const eventMap = await import("../src/packs/eventMap.ts");
const copy = await import("../src/copy.ts");

const expectedBindings = catalog.events
  .filter((event) => event.status === "active")
  .flatMap((event) => {
    const tiers = event.tier_range
      ? Array.from({ length: event.tier_range.max - event.tier_range.min + 1 }, (_, index) => event.tier_range!.min + index)
      : [null];
    return tiers.map((tier) => {
      const mapped = (event.id === "multi_kill" && tier === 2) || event.id === "first_blood";
      const notVoiced = event.media_mappable === false || event.voiced_default === false;
      const detectorUnavailable = ["first_blood", "enemy_missing", "risk_alert", "risk_revision"].includes(event.id);
      const state = notVoiced
          ? "not_voiced"
          : detectorUnavailable
            ? "detector_unavailable"
            : mapped
              ? "ready"
              : event.id === "multi_kill" && tier === 3
              ? "unmapped"
              : "unknown";
      return {
        event_id: event.id,
        tier,
        state,
        reason: event.id === "first_blood" ? "EVENT_UNAVAILABLE" : mapped ? "NONE" : notVoiced ? "NOT_VOICED" : detectorUnavailable ? "DETECTOR_UNAVAILABLE" : state === "unmapped" ? "MEDIA_UNMAPPED" : "MEDIA_STATE_UNKNOWN",
        resolved_from_tier: null,
        selection: null,
        variants: mapped ? [{ variant_id: "v1", clip_id: "clip-a", duration_ms: 1200, has_visual: true }] : [],
      };
    });
  });
const snapshot = {
  schema_version: "1.0.0",
  active: { install_id: "ins_0000000001", package_id: "voice.pack", revision: 2 },
  match_active: false,
  events: expectedBindings,
};

test("the media service match flag keeps the map readable and locks Listen", () => {
  const { match_active: _, ...rest } = snapshot;
  const parsed = eventMap.parseMediaEventMap({ ...rest, media_match_active: true });
  assert.ok(parsed);
  const row = parsed.bindings.find((item) => item.event_id === "multi_kill" && item.tier === 2)!;
  assert.equal(row.state, "ready");
  assert.equal(eventMap.eventMapPlayBlock(parsed, row, false, false), "AUDITION_MATCH_ACTIVE");
  assert.equal(eventMap.parseMediaEventMap({ ...snapshot, media_match_active: true })?.match_active, true);
});

test("parses the versioned host event map including unavailable and not-voiced catalog rows", () => {
  const parsed = eventMap.parseMediaEventMap(snapshot);
  assert.ok(parsed);
  assert.deepEqual(
    parsed.bindings.map(({ event_id, tier }) => [event_id, tier]),
    expectedBindings.map(({ event_id, tier }) => [event_id, tier]),
  );
  assert.equal(parsed.bindings.find((row) => row.event_id === "first_blood")?.state, "detector_unavailable");
  assert.equal(parsed.bindings.find((row) => row.event_id === "hero_respawn")?.state, "not_voiced");
  assert.equal(parsed.bindings.some((row) => row.event_id === "latency_probe"), false);
  const groups = eventMap.groupSoundRows(parsed.bindings);
  assert.equal(groups.flatMap(group => group.rows).length, expectedBindings.length);
  assert.equal(groups.find(group => group.id === "multi")?.rows[0]?.variants[0]?.duration_ms, 1200);
});

test("uses one version-aware variant parser for media event map 1.0 and 1.1", () => {
  const row = expectedBindings.find(item => item.event_id === "multi_kill" && item.tier === 2)!;
  const v11 = eventMap.parseMediaEventMap({
    ...snapshot,
    schema_version: "1.1.0",
    events: expectedBindings.map(item => item === row
      ? { ...item, variants: [{ ...item.variants[0], caption: "First voice" }] }
      : item),
  });
  assert.equal(v11?.schema_version, "1.1.0");
  assert.equal(v11?.bindings.find(item => item.event_id === "multi_kill" && item.tier === 2)?.variants[0]?.caption, "First voice");

  const legacy = eventMap.parseMediaEventMap(snapshot);
  assert.equal(legacy?.schema_version, "1.0.0");
  const captionOnLegacy = eventMap.parseMediaEventMap({
    ...snapshot,
    events: expectedBindings.map(item => item === row
      ? { ...item, variants: [{ ...item.variants[0], caption: "not in 1.0" }] }
      : item),
  });
  assert.equal(captionOnLegacy?.bindings.find(item => item.event_id === "multi_kill" && item.tier === 2)?.state, "unknown");
});

test("a missing, malformed, duplicate, or unexpected row degrades only that row to unknown", () => {
  const missingFirstBlood = eventMap.parseMediaEventMap({
    ...snapshot,
    events: snapshot.events.filter((row) => row.event_id !== "first_blood"),
  });
  assert.ok(missingFirstBlood);
  assert.equal(missingFirstBlood.bindings.find((row) => row.event_id === "first_blood")?.state, "unknown");
  assert.equal(missingFirstBlood.bindings.find((row) => row.event_id === "hero_respawn")?.state, "not_voiced");
  assert.equal(missingFirstBlood.bindings.find((row) => row.event_id === "multi_kill" && row.tier === 2)?.state, "ready");

  const duplicate = snapshot.events.find((row) => row.event_id === "multi_kill" && row.tier === 2)!;
  const duplicated = eventMap.parseMediaEventMap({ ...snapshot, events: [...snapshot.events, duplicate] });
  assert.ok(duplicated);
  assert.equal(duplicated.bindings.find((row) => row.event_id === "multi_kill" && row.tier === 2)?.state, "unknown");
  assert.equal(duplicated.bindings.find((row) => row.event_id === "multi_kill" && row.tier === 3)?.state, "unmapped");

  const malformed = eventMap.parseMediaEventMap({
    ...snapshot,
    events: snapshot.events.map((row) => row.event_id === "multi_kill" && row.tier === 2
      ? { ...row, variants: [{ variant_id: "v1", duration_ms: -1, has_visual: true }] }
      : row),
  });
  assert.ok(malformed);
  assert.equal(malformed.bindings.find((row) => row.event_id === "multi_kill" && row.tier === 2)?.state, "unknown");
  assert.equal(malformed.bindings.find((row) => row.event_id === "hero_respawn")?.state, "not_voiced");

  const unexpected = eventMap.parseMediaEventMap({
    ...snapshot,
    events: [...snapshot.events, {
      event_id: "future_event",
      tier: null,
      state: "ready",
      reason: "NONE",
      resolved_from_tier: null,
      selection: null,
      variants: [],
    }],
  });
  assert.ok(unexpected);
  assert.equal(unexpected.bindings.find((row) => row.event_id === "future_event")?.state, "unknown");
  assert.equal(unexpected.bindings.find((row) => row.event_id === "multi_kill" && row.tier === 2)?.state, "ready");
});

test("malformed top-level state remains unreadable without discarding row-local recovery", () => {
  assert.equal(eventMap.parseMediaEventMap(null), null);
  assert.equal(eventMap.parseMediaEventMap({ ...snapshot, schema_version: "2.0.0" }), null);
  assert.equal(eventMap.parseMediaEventMap({ ...snapshot, extra: true }), null);
  assert.equal(eventMap.parseMediaEventMap({ ...snapshot, match_active: "yes" }), null);
});

test("no active pack keeps detector and voice facts while blocking audition", () => {
  const parsed = eventMap.parseMediaEventMap({
    ...snapshot,
    active: null,
  });
  assert.ok(parsed);
  assert.equal(parsed.active, null);
  assert.equal(parsed.bindings.find((row) => row.event_id === "multi_kill" && row.tier === 2)?.state, "unknown");
  assert.equal(parsed.bindings.find((row) => row.event_id === "hero_respawn")?.state, "not_voiced");
  assert.equal(eventMap.eventMapPlayBlock(parsed, parsed.bindings[0], false, false), "AUDITION_NO_ACTIVE_PACK");
});

test("audition remains disabled through postgame and allows an unavailable detector preview", () => {
  const parsed = eventMap.parseMediaEventMap(snapshot);
  assert.ok(parsed);
  const ready = parsed.bindings.find((row) => row.event_id === "multi_kill" && row.tier === 2)!;
  for (const phase of ["draft", "pregame", "live", "post_game"] as const) {
    assert.equal(
      eventMap.eventMapPlayBlock(parsed, ready, false, false, eventMap.eventMapPhaseBlocksAudition(phase)),
      "AUDITION_MATCH_ACTIVE",
      phase,
    );
  }
  for (const phase of ["none", null] as const) {
    assert.equal(
      eventMap.eventMapPlayBlock(parsed, ready, false, false, eventMap.eventMapPhaseBlocksAudition(phase)),
      null,
    );
  }
  const firstBlood = parsed.bindings.find((row) => row.event_id === "first_blood")!;
  assert.equal(eventMap.eventMapPlayBlock(parsed, firstBlood, false, false), null, "a mapped unavailable detector may be auditioned");
  assert.equal(
    eventMap.eventMapPlayBlock(parsed, { ...firstBlood, variants: [] }, false, false),
    "AUDITION_EVENT_UNAVAILABLE",
    "without an auditionable variant the unavailable event has a specific refusal",
  );
});

test("match, missing host, unknown, invalid, unmapped, and busy playback have distinct refusal codes", () => {
  const parsed = eventMap.parseMediaEventMap(snapshot);
  assert.ok(parsed);
  const ready = parsed.bindings.find((row) => row.event_id === "multi_kill" && row.tier === 2)!;
  const unknown = parsed.bindings.find((row) => row.event_id === "multi_kill" && row.tier === 4)!;
  const unmapped = parsed.bindings.find((row) => row.event_id === "multi_kill" && row.tier === 3)!;
  const invalid = { ...ready, state: "invalid_media" as const, variants: [] };
  assert.equal(eventMap.eventMapPlayBlock({ ...parsed, match_active: true }, ready, false, false), "AUDITION_MATCH_ACTIVE");
  assert.equal(eventMap.eventMapPlayBlock(parsed, ready, false, false, true), "AUDITION_MATCH_ACTIVE");
  assert.equal(eventMap.eventMapPlayBlock(parsed, unknown, false, false), "AUDITION_MEDIA_STATE_UNKNOWN");
  assert.equal(eventMap.eventMapPlayBlock(parsed, unmapped, false, false), "AUDITION_UNMAPPED");
  assert.equal(eventMap.eventMapPlayBlock(parsed, invalid, false, false), "AUDITION_MEDIA_INVALID");
  assert.equal(eventMap.eventMapPlayBlock(parsed, ready, true, false), "CAPABILITY_UNAVAILABLE");
  assert.equal(eventMap.eventMapPlayBlock(parsed, ready, false, true), "AUDITION_BUSY");
});

test("obsolete eventMap refreshes cannot overwrite a newer snapshot or error", async () => {
  const oldSuccess = Promise.withResolvers<unknown>();
  const oldFailure = Promise.withResolvers<unknown>();
  const latest = Promise.withResolvers<unknown>();
  const commits: eventMap.EventMapLoadResult[] = [];
  const commit = (result: eventMap.EventMapLoadResult) => { commits.push(result); };

  eventMap.beginEventMapRefresh(() => oldSuccess.promise, commit)();
  eventMap.beginEventMapRefresh(() => oldFailure.promise, commit)();
  const dispose = eventMap.beginEventMapRefresh(() => latest.promise, commit);
  latest.resolve(snapshot);
  await latest.promise;

  oldSuccess.resolve({ ...snapshot, match_active: true });
  oldFailure.reject(new Error("old request"));
  await Promise.allSettled([oldSuccess.promise, oldFailure.promise]);

  assert.deepEqual(commits, [{ state: "ok", eventMap: eventMap.parseMediaEventMap(snapshot) }]);
  dispose();
});

test("refusal reasons are presented in Thai and English", () => {
  assert.equal(copy.auditionReason("AUDITION_MATCH_ACTIVE", "th"), "ทดลองฟังได้เมื่อออกจากแมตช์แล้ว");
  assert.equal(copy.auditionReason("AUDITION_NO_ACTIVE_PACK", "en"), "Activate a pack before auditioning a clip");
  assert.equal(copy.auditionReason("AUDITION_MEDIA_STATE_UNKNOWN", "en"), "Could not verify the active pack's clip");
  assert.equal(copy.auditionReason("AUDITION_MUTED", "th"), "เปิดเสียงก่อนทดลองฟัง");
  assert.equal(copy.auditionReason("NEW_CODE", "th"), copy.auditionReason("UNKNOWN", "th"));
});
