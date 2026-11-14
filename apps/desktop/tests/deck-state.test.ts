import assert from "node:assert/strict";
import { test } from "node:test";
// @ts-ignore Node's built-in TypeScript loader requires the explicit source extension.
import { deriveDeckState, type DeckStateInput } from "../src/deck/deckState.ts";
// @ts-ignore Node's built-in TypeScript loader requires the explicit source extension.
import type { CaptureStatus } from "../src/live/types.ts";
// @ts-ignore Node's built-in TypeScript loader requires the explicit source extension.
import type { Phase } from "../src/copy.ts";
// @ts-ignore Node's built-in TypeScript loader requires the explicit source extension.
import type { RuntimeStatus } from "../src/types.ts";

interface InputOptions {
  phase?: Phase;
  intakeState?: RuntimeStatus["intake"]["state"];
  snapshotAvailability?: DeckStateInput["snapshotAvailability"];
  capturePhase?: CaptureStatus["phase"];
  paused?: boolean | null;
  captureReason?: string | null;
  hostLost?: boolean;
}

function input(options: InputOptions = {}): DeckStateInput {
  const {
    phase = "live",
    intakeState = "receiving",
    snapshotAvailability = "fresh",
    capturePhase = "live",
    paused = false,
    captureReason = null,
    hostLost = false,
  } = options;
  return {
    runtime: {
      session: { phase, session_epoch: 1 },
      intake: { state: intakeState, port: 3000, generation: 1, last_admitted_age_ms: 20, reason: null },
    },
    snapshotAvailability,
    capture: {
      phase: capturePhase,
      paused,
      phase_reason: captureReason,
      phase_detail: null,
    },
    hostLost,
  };
}

test("derives every deck state from host, own snapshot and capture inputs", () => {
  const cases: [string, DeckStateInput, string][] = [
    ["standby for no game", input({ phase: "none" }), "standby"],
    ["standby during draft", input({ phase: "draft" }), "standby"],
    ["standby for unsupported game", input({ phase: "unsupported" }), "standby"],
    ["standby when intake is not receiving", input({ intakeState: "listening" }), "standby"],
    ["standby for unknown snapshot", input({ snapshotAvailability: "unknown" }), "standby"],
    ["standby for invalid snapshot", input({ snapshotAvailability: "invalid" }), "standby"],
    ["standby for unsupported snapshot", input({ snapshotAvailability: "unsupported" }), "standby"],
    ["live with fresh snapshot and live capture", input(), "live"],
    ["novision while calibrating", input({ capturePhase: "calibrating" }), "novision"],
    ["novision for stale capture", input({ capturePhase: "stale" }), "novision"],
    ["novision after capture loss", input({ capturePhase: "lost" }), "novision"],
    ["novision when capture is unsupported", input({ capturePhase: "unsupported" }), "novision"],
    ["novision when vision coverage is unavailable", input({ capturePhase: "coverage_unavailable" }), "novision"],
    ["novision when capture state is unknown", input({ capturePhase: "unknown" }), "novision"],
    ["novision when capture is paused", input({ paused: true, captureReason: "paused" }), "novision"],
    ["novision after user invalidates capture", input({ captureReason: "user_invalidated" }), "novision"],
    ["stale snapshot with live capture", input({ snapshotAvailability: "stale" }), "stale"],
    ["stale snapshot when intake also stopped", input({ snapshotAvailability: "stale", intakeState: "stale" }), "stale"],
    ["stale after host polling is lost", input({ hostLost: true }), "stale"],
    ["debrief after the game", input({ phase: "post_game" }), "debrief"],
  ];

  for (const [name, source, expected] of cases) {
    const result = deriveDeckState(source);
    assert.equal(result.state, expected, name);
    assert.ok(result.labelTh.length > 0, `${name}: Thai label`);
    assert.ok(result.labelEn.length > 0, `${name}: English label`);
    assert.ok(result.reason.th.length > 0 && result.reason.en.length > 0, `${name}: reason text`);
  }
});

test("debrief takes precedence over missing data, stale data, host loss and capture state", () => {
  const result = deriveDeckState(input({
    phase: "post_game",
    intakeState: "unavailable",
    snapshotAvailability: "stale",
    capturePhase: "live",
    paused: true,
    hostLost: true,
  }));
  assert.equal(result.state, "debrief");
});

test("stale own data beats a live capture state", () => {
  const result = deriveDeckState(input({ snapshotAvailability: "stale", capturePhase: "live", paused: false }));
  assert.equal(result.state, "stale");
});

test("unknown own snapshot cannot be promoted to live by a live capture", () => {
  const result = deriveDeckState(input({ snapshotAvailability: "unknown", capturePhase: "live", paused: false }));
  assert.equal(result.state, "standby");
});

test("a missing runtime status is standby even when a retained capture says live", () => {
  const result = deriveDeckState({ ...input(), runtime: null });
  assert.equal(result.state, "standby");
});
