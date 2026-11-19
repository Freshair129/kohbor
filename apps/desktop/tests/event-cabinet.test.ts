import assert from "node:assert/strict";
import { test } from "node:test";
import catalog from "../../../contracts/event-catalog.json" with { type: "json" };

const cabinet = await import("../src/cabinet.ts");
const copy = await import("../src/copy.ts");

const expectedBindings = catalog.events
  .filter((event) => event.status === "active" && event.voiced_default === true)
  .flatMap((event) => {
    const tiers = event.tier_range
      ? Array.from({ length: event.tier_range.max - event.tier_range.min + 1 }, (_, index) => event.tier_range!.min + index)
      : [null];
    return tiers.map((tier) => ({
      event_id: event.id,
      tier,
      mapped: event.id === "multi_kill" && tier === 2,
      duration_ms: event.id === "multi_kill" && tier === 2 ? 1200 : null,
      has_visual: event.id === "multi_kill" && tier === 2,
      state: event.id === "multi_kill" && tier === 2 ? "ready" : event.id === "multi_kill" && tier === 3 ? "unmapped" : "unknown",
    }));
  });
const snapshot = {
  active: { install_id: "ins_0000000001", package_id: "voice.pack", revision: 2 },
  match_active: false,
  bindings: expectedBindings,
};

test("parses catalog cabinet rows without inventing entries and groups tiers by event", () => {
  const parsed = cabinet.parseMediaCabinet(snapshot);
  assert.deepEqual(parsed, snapshot);
  const groups = cabinet.groupCabinetBindings(parsed.bindings);
  assert.deepEqual(groups.find((group) => group.eventId === "multi_kill")?.bindings.slice(0, 2), [
    snapshot.bindings[0], snapshot.bindings[1],
  ]);
  assert.equal(groups.length, catalog.events.filter((event) => event.status === "active" && event.voiced_default === true).length);
  assert.equal(cabinet.cabinetSummary(parsed), null, "unknown clip state keeps the summary unknown");
  const known = cabinet.parseMediaCabinet({
    ...snapshot,
    bindings: snapshot.bindings.map((binding) => binding.state === "unknown"
      ? { ...binding, state: "unmapped" as const }
      : binding),
  });
  assert.ok(known);
  assert.deepEqual(cabinet.cabinetSummary(known), { mapped: 1, total: expectedBindings.length });
  assert.equal(cabinet.cabinetMatchesPack(parsed, snapshot.active), true);
  assert.equal(cabinet.cabinetMatchesPack(parsed, null), false);
  assert.equal(cabinet.cabinetMatchesPack(parsed, { install_id: "ins_0000000002", revision: 2 }), false);
});

test("malformed or duplicate media state is unknown instead of a partial cabinet", () => {
  assert.equal(cabinet.parseMediaCabinet(null), null);
  assert.equal(cabinet.parseMediaCabinet({ ...snapshot, extra: true }), null);
  assert.equal(cabinet.parseMediaCabinet({ ...snapshot, match_active: "yes" }), null);
  assert.equal(cabinet.parseMediaCabinet({ ...snapshot, bindings: [snapshot.bindings[0], snapshot.bindings[0]] }), null);
  assert.equal(cabinet.parseMediaCabinet({ ...snapshot, bindings: snapshot.bindings.slice(1) }), null);
  assert.equal(cabinet.parseMediaCabinet({
    ...snapshot,
    bindings: [{ ...snapshot.bindings[0], mapped: false }],
  }), null);
});

test("no active pack remains explicit and blocks every audition", () => {
  const parsed = cabinet.parseMediaCabinet({
    active: null,
    match_active: false,
    bindings: expectedBindings.map((binding) => ({ ...binding, mapped: false, duration_ms: null, has_visual: false, state: "unknown" })),
  });
  assert.ok(parsed);
  assert.equal(parsed.active, null);
  assert.equal(cabinet.cabinetPlayBlock(parsed, parsed.bindings[0], false, false), "AUDITION_NO_ACTIVE_PACK");
});

test("audition is disabled through postgame and becomes available in the menu", () => {
  const parsed = cabinet.parseMediaCabinet(snapshot);
  assert.ok(parsed);
  for (const phase of ["draft", "pregame", "live", "post_game"] as const) {
    assert.equal(
      cabinet.cabinetPlayBlock(parsed, parsed.bindings[0], false, false, cabinet.cabinetPhaseBlocksAudition(phase)),
      "AUDITION_MATCH_ACTIVE",
      phase,
    );
  }
  for (const phase of ["none", null] as const) {
    assert.equal(
      cabinet.cabinetPlayBlock(parsed, parsed.bindings[0], false, false, cabinet.cabinetPhaseBlocksAudition(phase)),
      null,
    );
  }
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

  assert.deepEqual(commits, [{ state: "ok", cabinet: snapshot }]);
  dispose();
});

test("match, missing host, stale media, unmapped clips and busy playback have distinct refusal codes", () => {
  const parsed = cabinet.parseMediaCabinet(snapshot);
  assert.ok(parsed);
  assert.equal(cabinet.cabinetPlayBlock({ ...parsed, match_active: true }, parsed.bindings[0], false, false), "AUDITION_MATCH_ACTIVE");
  assert.equal(cabinet.cabinetPlayBlock(parsed, parsed.bindings[0], false, false, true), "AUDITION_MATCH_ACTIVE");
  assert.equal(cabinet.cabinetPlayBlock(parsed, parsed.bindings[2], false, false), "AUDITION_MEDIA_STATE_UNKNOWN");
  assert.equal(cabinet.cabinetPlayBlock(parsed, parsed.bindings[1], false, false), "AUDITION_UNMAPPED");
  assert.equal(cabinet.cabinetPlayBlock(parsed, parsed.bindings[0], true, false), "CAPABILITY_UNAVAILABLE");
  assert.equal(cabinet.cabinetPlayBlock(parsed, parsed.bindings[0], false, true), "AUDITION_BUSY");
});

test("refusal reasons are presented in Thai and English", () => {
  assert.equal(copy.cabinetReason("AUDITION_MATCH_ACTIVE", "th"), "ทดลองฟังได้เมื่อออกจากแมตช์แล้ว");
  assert.equal(copy.cabinetReason("AUDITION_NO_ACTIVE_PACK", "en"), "Activate a pack before auditioning a clip");
  assert.equal(copy.cabinetReason("AUDITION_MEDIA_STATE_UNKNOWN", "en"), "Could not verify the active pack's clip");
  assert.equal(copy.cabinetReason("AUDITION_MUTED", "th"), "เปิดเสียงก่อนทดลองฟัง");
  assert.equal(copy.cabinetReason("NEW_CODE", "th"), copy.cabinetReason("UNKNOWN", "th"));
});
