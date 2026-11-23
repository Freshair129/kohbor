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
  assert.deepEqual(detail.heroNames, ["lion"]);
  assert.equal(detail.durationS, 2460);
});
