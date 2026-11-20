import assert from "node:assert/strict";
import { test } from "node:test";
import catalog from "../../../contracts/event-catalog.json" with { type: "json" };

const cabinet = await import("../src/cabinet.ts");
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

test("parses the versioned host event map including unavailable and not-voiced catalog rows", () => {
  const parsed = cabinet.parseMediaCabinet(snapshot);
  assert.ok(parsed);
  assert.deepEqual(
    parsed.bindings.map(({ event_id, tier }) => [event_id, tier]),
    expectedBindings.map(({ event_id, tier }) => [event_id, tier]),
  );
  assert.equal(parsed.bindings.find((row) => row.event_id === "first_blood")?.state, "detector_unavailable");
  assert.equal(parsed.bindings.find((row) => row.event_id === "hero_respawn")?.state, "not_voiced");
  assert.equal(parsed.bindings.some((row) => row.event_id === "latency_probe"), false);
  const groups = cabinet.groupCabinetBindings(parsed.bindings);
  assert.equal(groups.length, catalog.events.filter((event) => event.status === "active").length);
  assert.equal(groups.find((group) => group.eventId === "multi_kill")?.bindings[0].variants[0].duration_ms, 1200);
  assert.equal(cabinet.cabinetSummary(parsed), null, "an unknown row keeps the summary unknown");
});

test("a missing, malformed, duplicate, or unexpected row degrades only that row to unknown", () => {
  const missingFirstBlood = cabinet.parseMediaCabinet({
    ...snapshot,
    events: snapshot.events.filter((row) => row.event_id !== "first_blood"),
  });
  assert.ok(missingFirstBlood);
  assert.equal(missingFirstBlood.bindings.find((row) => row.event_id === "first_blood")?.state, "unknown");
  assert.equal(missingFirstBlood.bindings.find((row) => row.event_id === "hero_respawn")?.state, "not_voiced");
  assert.equal(missingFirstBlood.bindings.find((row) => row.event_id === "multi_kill" && row.tier === 2)?.state, "ready");

  const duplicate = snapshot.events.find((row) => row.event_id === "multi_kill" && row.tier === 2)!;
  const duplicated = cabinet.parseMediaCabinet({ ...snapshot, events: [...snapshot.events, duplicate] });
  assert.ok(duplicated);
  assert.equal(duplicated.bindings.find((row) => row.event_id === "multi_kill" && row.tier === 2)?.state, "unknown");
  assert.equal(duplicated.bindings.find((row) => row.event_id === "multi_kill" && row.tier === 3)?.state, "unmapped");

  const malformed = cabinet.parseMediaCabinet({
    ...snapshot,
    events: snapshot.events.map((row) => row.event_id === "multi_kill" && row.tier === 2
      ? { ...row, variants: [{ variant_id: "v1", duration_ms: -1, has_visual: true }] }
      : row),
  });
  assert.ok(malformed);
  assert.equal(malformed.bindings.find((row) => row.event_id === "multi_kill" && row.tier === 2)?.state, "unknown");
  assert.equal(malformed.bindings.find((row) => row.event_id === "hero_respawn")?.state, "not_voiced");

  const unexpected = cabinet.parseMediaCabinet({
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
  assert.equal(cabinet.parseMediaCabinet(null), null);
  assert.equal(cabinet.parseMediaCabinet({ ...snapshot, schema_version: "2.0.0" }), null);
  assert.equal(cabinet.parseMediaCabinet({ ...snapshot, extra: true }), null);
  assert.equal(cabinet.parseMediaCabinet({ ...snapshot, match_active: "yes" }), null);
});

test("no active pack keeps detector and voice facts while blocking audition", () => {
  const parsed = cabinet.parseMediaCabinet({
    ...snapshot,
    active: null,
  });
  assert.ok(parsed);
  assert.equal(parsed.active, null);
  assert.equal(parsed.bindings.find((row) => row.event_id === "multi_kill" && row.tier === 2)?.state, "unknown");
  assert.equal(parsed.bindings.find((row) => row.event_id === "hero_respawn")?.state, "not_voiced");
  assert.equal(cabinet.cabinetPlayBlock(parsed, parsed.bindings[0], false, false), "AUDITION_NO_ACTIVE_PACK");
});

test("audition remains disabled through postgame and allows an unavailable detector preview", () => {
  const parsed = cabinet.parseMediaCabinet(snapshot);
  assert.ok(parsed);
  const ready = parsed.bindings.find((row) => row.event_id === "multi_kill" && row.tier === 2)!;
  for (const phase of ["draft", "pregame", "live", "post_game"] as const) {
    assert.equal(
      cabinet.cabinetPlayBlock(parsed, ready, false, false, cabinet.cabinetPhaseBlocksAudition(phase)),
      "AUDITION_MATCH_ACTIVE",
      phase,
    );
  }
  for (const phase of ["none", null] as const) {
    assert.equal(
      cabinet.cabinetPlayBlock(parsed, ready, false, false, cabinet.cabinetPhaseBlocksAudition(phase)),
      null,
    );
  }
  const firstBlood = parsed.bindings.find((row) => row.event_id === "first_blood")!;
  assert.equal(cabinet.cabinetPlayBlock(parsed, firstBlood, false, false), null, "a mapped unavailable detector may be auditioned");
  assert.equal(
    cabinet.cabinetPlayBlock(parsed, { ...firstBlood, variants: [] }, false, false),
    "AUDITION_EVENT_UNAVAILABLE",
    "without an auditionable variant the unavailable event has a specific refusal",
  );
});

test("match, missing host, unknown, invalid, unmapped, and busy playback have distinct refusal codes", () => {
  const parsed = cabinet.parseMediaCabinet(snapshot);
  assert.ok(parsed);
  const ready = parsed.bindings.find((row) => row.event_id === "multi_kill" && row.tier === 2)!;
  const unknown = parsed.bindings.find((row) => row.event_id === "multi_kill" && row.tier === 4)!;
  const unmapped = parsed.bindings.find((row) => row.event_id === "multi_kill" && row.tier === 3)!;
  const invalid = { ...ready, state: "invalid_media" as const, variants: [] };
  assert.equal(cabinet.cabinetPlayBlock({ ...parsed, match_active: true }, ready, false, false), "AUDITION_MATCH_ACTIVE");
  assert.equal(cabinet.cabinetPlayBlock(parsed, ready, false, false, true), "AUDITION_MATCH_ACTIVE");
  assert.equal(cabinet.cabinetPlayBlock(parsed, unknown, false, false), "AUDITION_MEDIA_STATE_UNKNOWN");
  assert.equal(cabinet.cabinetPlayBlock(parsed, unmapped, false, false), "AUDITION_UNMAPPED");
  assert.equal(cabinet.cabinetPlayBlock(parsed, invalid, false, false), "AUDITION_MEDIA_INVALID");
  assert.equal(cabinet.cabinetPlayBlock(parsed, ready, true, false), "CAPABILITY_UNAVAILABLE");
  assert.equal(cabinet.cabinetPlayBlock(parsed, ready, false, true), "AUDITION_BUSY");
});

test("obsolete cabinet refreshes cannot overwrite a newer snapshot or error", async () => {
  const oldSuccess = Promise.withResolvers<unknown>();
  const oldFailure = Promise.withResolvers<unknown>();
  const latest = Promise.withResolvers<unknown>();
  const commits: cabinet.CabinetLoadResult[] = [];
  const commit = (result: cabinet.CabinetLoadResult) => { commits.push(result); };

  cabinet.beginCabinetRefresh(() => oldSuccess.promise, commit)();
  cabinet.beginCabinetRefresh(() => oldFailure.promise, commit)();
  const dispose = cabinet.beginCabinetRefresh(() => latest.promise, commit);
  latest.resolve(snapshot);
  await latest.promise;

  oldSuccess.resolve({ ...snapshot, match_active: true });
  oldFailure.reject(new Error("old request"));
  await Promise.allSettled([oldSuccess.promise, oldFailure.promise]);

  assert.deepEqual(commits, [{ state: "ok", cabinet: cabinet.parseMediaCabinet(snapshot) }]);
  dispose();
});

test("refusal reasons are presented in Thai and English", () => {
  assert.equal(copy.cabinetReason("AUDITION_MATCH_ACTIVE", "th"), "ทดลองฟังได้เมื่อออกจากแมตช์แล้ว");
  assert.equal(copy.cabinetReason("AUDITION_NO_ACTIVE_PACK", "en"), "Activate a pack before auditioning a clip");
  assert.equal(copy.cabinetReason("AUDITION_MEDIA_STATE_UNKNOWN", "en"), "Could not verify the active pack's clip");
  assert.equal(copy.cabinetReason("AUDITION_MUTED", "th"), "เปิดเสียงก่อนทดลองฟัง");
  assert.equal(copy.cabinetReason("NEW_CODE", "th"), copy.cabinetReason("UNKNOWN", "th"));
});
