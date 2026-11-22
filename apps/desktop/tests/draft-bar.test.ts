import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";
import { DRAFT_HEIGHT, DRAFT_WIDTH, draftBarPresentation, parseDraftBarPacket, parseSlotRoster } from "../src/deck/draftBarModel.ts";
import { addResourceSample, emptyResourceSummary, parseResourceMetrics } from "../src/live/state.ts";

function packet({ active = true, hasFrame = true, identified = hasFrame, extraBytes = 0 } = {}) {
  const slots = Array.from({ length: 10 }, (_, slot) => ({
    slot, team: slot < 5 ? "radiant" : "dire",
    hero_id: identified && slot === 0 ? "npc_dota_hero_nevermore" : null,
    confidence: identified && slot === 0 ? 0.93 : 0,
    reason: identified && slot === 0 ? "identified" : hasFrame ? "low_confidence" : "awaiting_frame",
  }));
  const meta = new TextEncoder().encode(JSON.stringify({ active, has_frame: hasFrame, detail: null, slots }));
  const buffer = new ArrayBuffer(4 + meta.length + (hasFrame ? DRAFT_WIDTH * DRAFT_HEIGHT * 4 : 0) + extraBytes);
  new DataView(buffer).setUint32(0, meta.length, true);
  new Uint8Array(buffer, 4).set(meta);
  return buffer;
}

test("draft deck shows five slots on each side of the clock with names or unknown tooltips", () => {
  const view = draftBarPresentation(parseDraftBarPacket(packet()), false, "en");
  assert.ok(view);
  assert.deepEqual(view.slots.map((slot) => slot.column), [1, 2, 3, 4, 5, 7, 8, 9, 10, 11]);
  assert.deepEqual(view.slots[0], { column: 1, team: "radiant", text: "Shadow Fiend", title: "Identified · 93%" });
  assert.deepEqual(view.slots[5], { column: 7, team: "dire", text: "?", title: "No confident match · 0%" });
});

test("draft strip hides outside admitted draft phases or when the host is lost", () => {
  const off = parseDraftBarPacket(packet({ active: false, hasFrame: false }));
  assert.ok(off);
  assert.equal(draftBarPresentation(off, false, "en"), null);
  assert.equal(draftBarPresentation(parseDraftBarPacket(packet()), true, "en"), null);
  const pending = draftBarPresentation(parseDraftBarPacket(packet({ hasFrame: false })), false, "en");
  assert.ok(pending);
  assert.equal(pending.slots[0]?.text, "?");
});

test("private packet rejects mismatched frame size and identities without an admitted image", () => {
  assert.equal(parseDraftBarPacket(packet({ extraBytes: 1 })), null);
  assert.equal(parseDraftBarPacket(packet({ active: false })), null);
  assert.equal(parseDraftBarPacket(packet({ hasFrame: false, identified: true })), null);
});

test("resource readout aggregates the draft cycle cost with no slot fields", () => {
  const metrics = parseResourceMetrics({
    working_set_bytes: null, cpu_percent: null, gpu_local_bytes: null, gpu_group_bytes: null,
    gpu_util: null, capture_fps: null, frame_age_ms: null, draft_bar_cycle_ms_mean: 2.5,
    hero_id: "npc_dota_hero_nevermore", player_name: "synthetic player",
  });
  assert.ok(metrics);
  assert.equal(metrics.draftBarCycleMs, 2.5);
  assert.equal(Object.hasOwn(metrics, "hero_id"), false);
  assert.equal(Object.hasOwn(metrics, "player_name"), false);
  assert.equal(addResourceSample(emptyResourceSummary(), metrics).draftBarCycleMs.mean, 2.5);
});

test("only the Live deck mounts the draft canvas and slot labels", () => {
  const source = (path: string) => readFileSync(new URL(`../src/${path}`, import.meta.url), "utf8");
  assert.match(source("deck/Deck.tsx"), /<DraftBarStrip frame={draftBar} locale={locale}/);
  const strip = source("deck/DraftBarStrip.tsx");
  assert.match(strip, /frame\?\.bgra\s*\? <canvas /);
  assert.match(strip, /view\.slots\.map\(.*[\s\S]*?title={slot\.title}[\s\S]*?{slot\.text}/);
  for (const path of ["OverlayStage.tsx", "OverlayChip.tsx", "HudStage.tsx"]) {
    assert.doesNotMatch(source(path), /DraftBarStrip|draft_bar_read|draftBarModel/);
  }
});

function field(value: unknown, source: string, reason: string | null = null) {
  return { value, confidence: value === null ? 0 : 0.9, observed_at_ms: value === null ? null : 1795400000000, source, reason };
}

function roster() {
  return {
    slots: Array.from({ length: 10 }, (_, slot) => ({
      slot, side: slot < 5 ? "radiant" : "dire", index: slot % 5,
      hero: field(slot === 7 ? "npc_dota_hero_lion" : null, "draft_bar_5b", slot === 7 ? null : "low_confidence"),
      player_colour: field(slot === 3 ? null : { name: "blue", rgb: [51, 117, 252] }, "draft_bar_5d", slot === 3 ? "no_player_colour" : null),
      lane_role: field(slot === 9 ? "hard_support" : null, "draft_bar_5e", slot === 9 ? null : "enemy_team"),
      player_name: field(null, "draft_bar_5c", "not_built"),
    })),
    own_slot: field(7, "gsi_and_5b"),
  };
}

test("the deck reads the per-slot roster and drops only a malformed roster", () => {
  const withRoster = (value: unknown) => {
    const slots = Array.from({ length: 10 }, (_, slot) => ({
      slot, team: slot < 5 ? "radiant" : "dire", hero_id: null, confidence: 0, reason: "awaiting_frame",
    }));
    const meta = new TextEncoder().encode(JSON.stringify({ active: true, has_frame: false, detail: null, slots, roster: value }));
    const buffer = new ArrayBuffer(4 + meta.length);
    new DataView(buffer).setUint32(0, meta.length, true);
    new Uint8Array(buffer, 4).set(meta);
    return parseDraftBarPacket(buffer);
  };
  const frame = withRoster(roster());
  assert.ok(frame?.roster);
  assert.equal(frame.roster.own_slot.value, 7);
  assert.equal(frame.roster.slots[7]!.hero.value, "npc_dota_hero_lion");
  assert.equal(frame.roster.slots[9]!.lane_role.value, "hard_support");
  assert.equal(frame.roster.slots[3]!.player_colour.reason, "no_player_colour");
  assert.equal(frame.roster.slots[0]!.player_name.reason, "not_built");

  const broken = roster();
  (broken.slots[2] as Record<string, unknown>).lane_role = field("jungle", "draft_bar_5e");
  const kept = withRoster(broken);
  assert.ok(kept, "the strip still parses");
  assert.equal(kept.roster, null);
  assert.equal(parseSlotRoster({ slots: [], own_slot: field(null, "gsi_and_5b") }), null);
  assert.equal(withRoster(null)?.roster, null);
});
