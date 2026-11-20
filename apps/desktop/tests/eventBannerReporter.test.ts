import assert from "node:assert/strict";
import { test } from "node:test";
import { createPlaybackReporter } from "../src/playbackReporter.ts";

test("a late start timeout for an earlier playback is ignored after that playback started, even when a newer one also started", () => {
  const sent: string[] = [];
  const report = createPlaybackReporter((id, outcome, _code, reason) => sent.push(`${id}:${outcome}:${reason}`));
  report("A", "started", null, "NONE");
  report("B", "started", null, "NONE");
  assert.equal(report("A", "failed", null, "VIDEO_START_TIMEOUT"), false);
  assert.equal(report("B", "failed", null, "VIDEO_START_TIMEOUT"), false);
  assert.deepEqual(sent, ["A:started:NONE", "B:started:NONE"]);
});

test("a playback that never started still reports one timeout", () => {
  const sent: string[] = [];
  const report = createPlaybackReporter((id, outcome, _code, reason) => sent.push(`${id}:${outcome}:${reason}`));
  report("A", "failed", null, "VIDEO_START_TIMEOUT");
  report("A", "failed", null, "VIDEO_START_TIMEOUT");
  report(null, "failed", null, "VIDEO_START_TIMEOUT");
  assert.deepEqual(sent, ["A:failed:VIDEO_START_TIMEOUT"]);
});
