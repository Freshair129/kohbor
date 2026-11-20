import assert from "node:assert/strict";
import test from "node:test";
import {
  clearMatchRecap,
  emptyMatchRecap,
  formatGameClock,
  loadMatchRecap,
  parseMatchRecap,
} from "../src/packs/recapModel.ts";

function outcome(match: number, instance: string, values: Record<string, unknown> = {}) {
  return {
    schema_version: "1.1.0",
    instance_id: instance,
    event_type: "hero_kill",
    session_epoch: match,
    audio_class: "ordinary",
    outcome: "admitted",
    playback_id: `pb_${String(match).padStart(10, "0")}`,
    package_revision: 7,
    decided_monotonic_ms: 1234,
    tier: 3,
    variant_id: "kill_03",
    clip_id: "clip-03",
    selection: "shuffle",
    game_clock_seconds: 91,
    match_id: `local_match_${match}`,
    playback_end: "finished",
    ...values,
  };
}

test("groups played events by local match and keeps dropped counts separate", () => {
  const recap = parseMatchRecap({
    schema_version: "1.0.0",
    matches: [
      { match_id: "local_match_2", events: [outcome(2, "event_2_1"), outcome(2, "event_2_2", { outcome: "channel_busy", playback_id: null, playback_end: null })] },
      { match_id: "local_match_1", events: [outcome(1, "event_1_1", { game_clock_seconds: null, playback_end: "cut" })] },
    ],
  });

  assert.equal(recap.readable, true);
  assert.equal(recap.matches.length, 2);
  assert.equal(recap.matches[0]?.match_id, "local_match_2");
  assert.equal(recap.matches[0]?.playedEvents.length, 1);
  assert.equal(recap.matches[0]?.droppedCount, 1);
  assert.equal(recap.matches[1]?.playedEvents[0]?.playback_end, "cut");
  assert.equal(recap.matches[1]?.playedEvents[0]?.game_clock_seconds, null);
});

test("empty recap gives a readable empty state", () => {
  const recap = parseMatchRecap({ schema_version: "1.0.0", matches: [] });
  assert.deepEqual(recap, emptyMatchRecap());
  assert.equal(recap.matches.length, 0);
});

test("invalid records are skipped and a wrong file schema is unreadable", () => {
  const skipped = parseMatchRecap({
    schema_version: "1.0.0",
    matches: [{ match_id: "local_match_1", events: [outcome(1, "event_1_1", { game_clock_seconds: -1 })] }],
  });
  assert.equal(skipped.skipped, 1);
  assert.equal(skipped.matches[0]?.playedEvents.length, 0);
  assert.equal(parseMatchRecap({ schema_version: "2.0.0", matches: [] }).readable, false);
});

test("saved matches skip demos, auditions, and outcomes attributed to another match", () => {
  const recap = parseMatchRecap({ schema_version: "1.0.0", matches: [{
    match_id: "local_match_1",
    events: [outcome(1, "ev_g18446744073709551615_s1_0"), outcome(1, "event_1_2", { event_type: "audition" }),
      outcome(2, "event_2_1"), outcome(1, "event_1_4")],
  }] });
  assert.equal(recap.skipped, 3);
  assert.deepEqual(recap.matches[0]?.playedEvents.map(event => event.instance_id), ["event_1_4"]);
});

test("clock formatting preserves unknown and formats known game time", () => {
  assert.equal(formatGameClock(null), "—");
  assert.equal(formatGameClock(91), "1:31");
});

test("host helpers use the read and clear commands", async () => {
  const commands: string[] = [];
  const command = async (name: "match_recap" | "clear_match_recap") => {
    commands.push(name);
    return name === "match_recap" ? { schema_version: "1.0.0", matches: [] } : { cleared: true };
  };
  const recap = await loadMatchRecap(command);
  assert.equal(recap.readable, true);
  await clearMatchRecap(command);
  assert.deepEqual(commands, ["match_recap", "clear_match_recap"]);
});
