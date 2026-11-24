import assert from "node:assert/strict";
import test from "node:test";
import {
  DEFAULT_MINIMAP_RECORDER_CONFIG,
  formatRecordingBytes,
  parseMinimapRecorderStatus,
  recordingIndicatorState,
  shouldShowRecordingIndicator,
} from "../src/lab/minimapRecorderModel.ts";

test("recorder consent defaults off at two frames per second with a five GB cap", () => {
  assert.deepEqual(DEFAULT_MINIMAP_RECORDER_CONFIG, {
    enabled: false,
    sampling_rate_fps: 2,
    total_cap_bytes: 5_000_000_000,
  });
});

test("status parsing rejects unsupported sampling rates and preserves unknown metadata", () => {
  const status = parseMinimapRecorderStatus({
    available: true,
    enabled: true,
    sampling_rate_fps: 2,
    total_cap_bytes: 5_000_000_000,
    per_match_frame_cap_bytes: 150_000_000,
    recording: true,
    recordings_count: 1,
    total_bytes: 128,
    dropped_frames: 3,
    stop_reason: null,
    matches: [{
      match_hash: "a".repeat(64),
      frame_count: 2,
      frame_bytes: 128,
      total_bytes: 256,
      ally_hero_ids: ["npc_dota_hero_axe", "npc_dota_hero_bane"],
      enemy_hero_ids: ["npc_dota_hero_lina", "npc_dota_hero_lion"],
    }],
  });
  assert.ok(status);
  assert.deepEqual(status.matches[0].ally_hero_ids, ["npc_dota_hero_axe", "npc_dota_hero_bane"]);
  assert.deepEqual(status.matches[0].enemy_hero_ids, ["npc_dota_hero_lina", "npc_dota_hero_lion"]);
  assert.equal(parseMinimapRecorderStatus({
    available: true,
    enabled: true,
    sampling_rate_fps: 6,
    total_cap_bytes: 1000,
    per_match_frame_cap_bytes: 150_000_000,
    recording: false,
    recordings_count: 0,
    total_bytes: 0,
    dropped_frames: 0,
    stop_reason: null,
    matches: [],
  }), null);
});

test("the indicator appears only when consent and active recording are both true", () => {
  const status = parseMinimapRecorderStatus({
    available: true,
    enabled: true,
    sampling_rate_fps: 2,
    total_cap_bytes: 5000,
    per_match_frame_cap_bytes: 150_000_000,
    recording: true,
    recordings_count: 0,
    total_bytes: 0,
    dropped_frames: 0,
    stop_reason: null,
    matches: [],
  });
  assert.ok(status);
  assert.equal(shouldShowRecordingIndicator(status), true);
  assert.equal(recordingIndicatorState(status), "recording");
  assert.equal(recordingIndicatorState({ ...status, recording: false }), "enabled");
  assert.equal(recordingIndicatorState({ ...status, enabled: false }), null);
  assert.equal(shouldShowRecordingIndicator({ ...status, enabled: false }), false);
  assert.equal(shouldShowRecordingIndicator({ ...status, recording: false }), false);
  assert.equal(shouldShowRecordingIndicator(null), false);
});

test("recording byte totals use decimal storage units", () => {
  assert.equal(formatRecordingBytes(0), "0 B");
  assert.equal(formatRecordingBytes(1500), "1.5 kB");
  assert.equal(formatRecordingBytes(1_000_000), "1.0 MB");
});
