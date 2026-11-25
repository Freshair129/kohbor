import assert from "node:assert/strict";
import { test } from "node:test";
import { replayListViewModel, replayMetadataFromBundle, replayStatusLabel } from "../src/replay/replayLibraryModel.ts";
import type { ReplayLibraryEntry, ReplayStatus } from "../src/replay/replayLibraryClient";
import type { MatchBundle } from "../src/replay/matchBundle";

const states = {
  available: "Available", queued: "Queued", parsing: "Parsing", ready: "Ready",
  failed: (reason: string) => `Failed (${reason})`, needs_pack: "Pack required",
};
const copy = {
  states,
  source: { dota: "Dota", manual: "Manual" },
  unknown: "Unknown",
  noDate: "Unknown",
  dateAdded: (date: string) => `Added ${date}`,
  duration: (duration: string) => `Duration ${duration}`,
  heroes: (names: string[]) => names.join(", "),
};

function entry(match_id: string, status: ReplayStatus): ReplayLibraryEntry {
  return {
    match_id, source: "dota_replays_folder", source_path: "private-source-path", dem_sha256: null,
    engine_version: null, engine_sha256: null, parse_time_unix_ms: null, status,
    bundle_bytes: 0, storage_bytes: 0, created_at_unix_ms: Date.UTC(2026, 0, 3),
    match_date_utc: null, own_hero: null, duration_s: null, patch: "7.42",
    source_exists: true, bundle_schema: status.state === "ready" ? "3.0" : null,
  };
}

test("library view model exposes state chips and only valid row actions", () => {
  const rows = replayListViewModel([
    entry("1", { state: "available" }),
    entry("2", { state: "queued" }),
    entry("3", { state: "parsing" }),
    entry("4", { state: "ready" }),
    entry("5", { state: "failed", reason: "timeout" }),
    entry("6", { state: "needs_pack" }),
  ], {}, copy, "en");

  assert.deepEqual(rows.map(row => row.statusLabel), ["Available", "Queued", "Parsing", "Ready", "Failed (timeout)", "Pack required"]);
  assert.deepEqual(rows.map(row => [row.canOpen, row.canParse]), [
    [false, true], [false, false], [false, false], [true, false], [false, true], [false, false],
  ]);
  assert.equal(rows[0]?.addedDateLabel, "Added Jan 3, 2026");
  assert.equal(rows[0]?.matchDateLabel, "Unknown");
  assert.equal(rows[0]?.heroesLabel, "Unknown");
  assert.equal(rows[0]?.durationLabel, "Unknown");
  assert.equal(rows[0]?.sourceLabel, "Dota");
});

test("unsupported patch discovered while opening overrides the row chip to needs_pack", () => {
  const rows = replayListViewModel(
    [entry("1", { state: "ready" })], {}, copy, "en", { "1": { state: "needs_pack" } },
  );
  assert.equal(rows[0]?.statusLabel, "Pack required");
  assert.equal(rows[0]?.canOpen, false);
  assert.equal(rows[0]?.canParse, false);
});

test("state chip text includes the stable failure reason", () => {
  assert.equal(replayStatusLabel({ state: "failed", reason: "bad_output" }, states), "Failed (bad_output)");
});

test("ready rows show their bundle schema and enable re-parse only when the source exists", () => {
  const ready = entry("ready", { state: "ready" });
  const missing = { ...entry("missing", { state: "ready" }), source_exists: false, bundle_schema: "3.1" as const };
  const rows = replayListViewModel([ready, missing], {}, copy, "en");
  assert.equal(rows[0]?.showReparse, true);
  assert.equal(rows[0]?.bundleSchema, "3.0");
  assert.equal(rows[0]?.canReparse, true);
  assert.equal(rows[1]?.showReparse, true);
  assert.equal(rows[1]?.bundleSchema, "3.1");
  assert.equal(rows[1]?.canReparse, false);
});

test("queued re-parse shows its job state and disables another re-parse while the old bundle stays openable", () => {
  const row = replayListViewModel(
    [entry("1", { state: "ready" })], {}, copy, "en", { "1": { state: "parsing" } },
  )[0]!;
  assert.equal(row.status.state, "parsing");
  assert.equal(row.canOpen, true);
  assert.equal(row.showReparse, true);
  assert.equal(row.canReparse, false);
});

test("bundle metadata can be displayed without pretending an unsupported patch has a hero pack", () => {
  const bundle = {
    match: {
      meta: { end_time_utc: "2026-01-02T03:04:05Z" },
      heroes: [{ id: "npc_dota_hero_lion" }],
    },
    durationS: 2460,
  } as unknown as MatchBundle;
  const detail = replayMetadataFromBundle(bundle, null);
  assert.equal(detail.matchDate, Date.parse("2026-01-02T03:04:05Z"));
  assert.equal(detail.ownHero, null);
  assert.deepEqual(detail.heroNames, ["lion"]);
  assert.equal(detail.durationS, 2460);
});

test("list view reads host metadata and marks a ready replay needs_pack for an unknown patch", () => {
  const row = replayListViewModel([{
    ...entry("77", { state: "ready" }),
    match_date_utc: "2026-11-23T21:40:00Z",
    own_hero: "npc_dota_hero_lion",
    duration_s: 1800,
    patch: "7.43",
  }], {}, copy, "en")[0]!;
  assert.equal(row.matchDateLabel, "Nov 24, 2026");
  assert.equal(row.heroesLabel, "lion");
  assert.equal(row.durationLabel, "Duration 30:00");
  assert.equal(row.status.state, "needs_pack");
  assert.equal(row.canOpen, false);
});
