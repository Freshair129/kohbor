import assert from "node:assert/strict";
import { test } from "node:test";
import { deriveHeaderState, nextShellTab } from "../src/shell/state.ts";
import { copy } from "../src/copy.ts";

test("idle capture header says waiting instead of claiming capture is running", () => {
  const text = copy("th");
  const labels = { phases: text.phases, staleSuffix: text.readiness.staleSuffix,
    capturePhases: text.shell.capturePhases, capturePaused: text.shell.capturePaused, captureRunning: text.shell.captureRunning };
  for (const paused of [true, false, null]) {
    assert.deepEqual(deriveHeaderState(null, { phase: "idle", paused }, false, labels).capture, { label: "รอเกม", tone: "unknown" });
  }
});

test("tab keyboard navigation moves, wraps and honors Home and End", () => {
  assert.equal(nextShellTab("live", "ArrowRight"), "packs");
  assert.equal(nextShellTab("live", "ArrowLeft"), "status");
  assert.equal(nextShellTab("packs", "Home"), "live");
  assert.equal(nextShellTab("live", "End"), "status");
  assert.equal(nextShellTab("live", "Enter"), null);
});

test("header data stays visibly unknown before status and capture arrive", () => {
  const state = deriveHeaderState(null, null, false);
  assert.deepEqual(state, {
    readiness: { label: "—", tone: "unknown" },
    gameAge: "—",
    capture: { label: "—", tone: "unknown" },
  });
});
