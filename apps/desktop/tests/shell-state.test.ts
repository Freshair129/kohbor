import assert from "node:assert/strict";
import { test } from "node:test";
import { deriveHeaderState, nextShellTab } from "../src/shell/state.ts";

test("tab keyboard navigation moves, wraps and honors Home and End", () => {
  assert.equal(nextShellTab("live", "ArrowRight"), "packs");
  assert.equal(nextShellTab("live", "ArrowLeft"), "status");
  assert.equal(nextShellTab("settings", "Home"), "live");
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
