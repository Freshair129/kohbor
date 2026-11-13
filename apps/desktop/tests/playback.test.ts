import assert from "node:assert/strict";
import { test } from "node:test";

const playback = await import("../src/playback.ts");

test("playback module exports the pure event API", () => {
  assert.equal(typeof playback.parsePlaybackEvent, "function");
  assert.equal(typeof playback.reducePlayback, "function");
  assert.equal(typeof playback.tickPlayback, "function");
});

if (!playback) {
  test("playback behavior tests require the implementation", { skip: true }, () => {});
} else {
  const {
    createPlaybackState,
    parsePlaybackEvent,
    reducePlayback,
    tickPlayback,
  } = playback;

  const started = (overrides: Record<string, unknown> = {}) => ({
    kind: "started",
    playback_id: "pb_00000001",
    class: "critical",
    event_type: "multi_kill",
    tier: 3,
    visual_url: "kohbor-media://ins_00000001/video/multi_kill-t3.webm",
    audio_duration_ms: 1200,
    visual_tail_ms: 0,
    at_ms: 1000,
    ...overrides,
  });

  const ended = (playback_id = "pb_00000001", overrides: Record<string, unknown> = {}) => ({
    kind: "ended",
    playback_id,
    reason: "finished",
    at_ms: 2200,
    ...overrides,
  });

  test("parses valid started and ended events", () => {
    assert.deepEqual(parsePlaybackEvent(started()), started());
    assert.deepEqual(parsePlaybackEvent(ended()), ended());
  });

  test("accepts every defined playback class", async (t) => {
    for (const playbackClass of ["critical", "correction", "notice", "ordinary", "voice"]) {
      await t.test(playbackClass, () => {
        assert.equal(parsePlaybackEvent(started({ class: playbackClass }))?.kind, "started");
      });
    }
  });

  test("accepts every defined end reason", async (t) => {
    for (const endReason of [
      "finished",
      "cancelled_preempted",
      "cancelled_session",
      "cancelled_revision",
      "cancelled_muted",
      "cancelled_device_lost",
    ]) {
      await t.test(endReason, () => {
        assert.deepEqual(parsePlaybackEvent(ended("pb_00000001", { reason: endReason })), {
          kind: "ended",
          playback_id: "pb_00000001",
          reason: endReason,
          at_ms: 2200,
        });
      });
    }
  });

  test("accepts null event metadata and a null visual", () => {
    assert.deepEqual(
      parsePlaybackEvent(started({ event_type: null, tier: null, visual_url: null })),
      started({ event_type: null, tier: null, visual_url: null }),
    );
  });

  test("accepts catalog event ids including health_danger and other events", () => {
    for (const event_type of ["health_danger", "risk_alert", "risk_revision", "multi_kill"]) {
      assert.equal(parsePlaybackEvent(started({ event_type }))?.kind, "started");
    }
  });

  test("a new started event cuts and replaces the current visual", () => {
    const first = reducePlayback(createPlaybackState(), started());
    const secondEvent = started({
      playback_id: "pb_00000002",
      visual_url: "kohbor-media://ins_00000002/video/kill.webm",
      at_ms: 1200,
    });
    const second = reducePlayback(first, secondEvent);

    assert.equal(second.currentPlaybackId, "pb_00000002");
    assert.equal(second.visual?.playback_id, "pb_00000002");
    assert.equal(second.visual?.visual_url, "kohbor-media://ins_00000002/video/kill.webm");
  });

  test("ended for the current playback hides its visual immediately", () => {
    const state = reducePlayback(createPlaybackState(), started());
    const endedState = reducePlayback(state, ended());

    assert.equal(endedState.visual, null);
    assert.equal(endedState.failSafeAtMs, null);
    assert.equal(endedState.currentPlaybackId, "pb_00000001");
  });

  test("ended for another playback id is ignored", () => {
    const state = reducePlayback(createPlaybackState(), started());
    const result = reducePlayback(state, ended("pb_00000002"));

    assert.strictEqual(result, state);
    assert.equal(result.visual?.playback_id, "pb_00000001");
  });

  test("a duplicate started event for the current id cannot restart or replace it", () => {
    const state = reducePlayback(createPlaybackState(), started());
    const result = reducePlayback(
      state,
      started({ visual_url: "kohbor-media://ins_00000002/video/duplicate.webm", at_ms: 1500 }),
    );

    assert.strictEqual(result, state);
  });

  test("a duplicate started event cannot resurrect a visual after ended", () => {
    const state = reducePlayback(
      reducePlayback(createPlaybackState(), started()),
      ended(),
    );
    const result = reducePlayback(state, started({ at_ms: 3000 }));

    assert.strictEqual(result, state);
    assert.equal(result.visual, null);
  });

  test("a started event without a visual cuts the prior visual and shows nothing", () => {
    const state = reducePlayback(createPlaybackState(), started());
    const result = reducePlayback(
      state,
      started({ playback_id: "pb_00000002", visual_url: null, at_ms: 1100 }),
    );

    assert.equal(result.currentPlaybackId, "pb_00000002");
    assert.equal(result.visual, null);
    assert.equal(result.failSafeAtMs, null);
  });

  test("a voice started event cuts any prior visual and shows nothing", () => {
    const state = reducePlayback(createPlaybackState(), started());
    const result = reducePlayback(
      state,
      started({ playback_id: "pb_00000002", class: "voice", at_ms: 1100 }),
    );

    assert.equal(result.currentPlaybackId, "pb_00000002");
    assert.equal(result.visual, null);
  });

  test("the fail-safe keeps the visual until one millisecond before its boundary", () => {
    const state = reducePlayback(
      createPlaybackState(),
      started({ at_ms: 1000, audio_duration_ms: 1200, visual_tail_ms: 400 }),
    );
    const beforeBoundary = tickPlayback(state, 3099);

    assert.strictEqual(beforeBoundary, state);
    assert.equal(beforeBoundary.visual?.playback_id, "pb_00000001");
  });

  test("the fail-safe hides the visual at audio plus tail plus 500 ms", () => {
    const state = reducePlayback(
      createPlaybackState(),
      started({ at_ms: 1000, audio_duration_ms: 1200, visual_tail_ms: 400 }),
    );
    const atBoundary = tickPlayback(state, 3100);

    assert.equal(atBoundary.visual, null);
    assert.equal(atBoundary.failSafeAtMs, null);
    assert.equal(atBoundary.currentPlaybackId, "pb_00000001");
  });

  const malformedEvents: [string, unknown][] = [
    ["unknown kind", { ...started(), kind: "queued" }],
    ["short playback id", { ...started(), playback_id: "short" }],
    ["playback id outside the allowed character set", { ...started(), playback_id: "pb!000001" }],
    ["long playback id", { ...started(), playback_id: "x".repeat(65) }],
    ["negative start time", { ...started(), at_ms: -1 }],
    ["fractional start time", { ...started(), at_ms: 1.5 }],
    ["fractional duration", { ...started(), audio_duration_ms: 1.5 }],
    ["negative duration", { ...started(), audio_duration_ms: -1 }],
    ["fractional visual tail", { ...started(), visual_tail_ms: 0.5 }],
    ["negative tail", { ...started(), visual_tail_ms: -1 }],
    ["negative tier", { ...started(), tier: -1 }],
    ["fractional tier", { ...started(), tier: 2.5 }],
    ["uppercase event type", { ...started(), event_type: "Health_danger" }],
    ["event type longer than 80 characters", { ...started(), event_type: "a".repeat(81) }],
    ["empty event type", { ...started(), event_type: "" }],
    ["invalid visual scheme", { ...started(), visual_url: "https://example.test/clip.webm" }],
    ["missing required duration", { ...started(), audio_duration_ms: undefined }],
    ["unknown class", { ...started(), class: "urgent" }],
    ["unknown ended reason", { ...ended(), reason: "timeout" }],
    ["negative ended time", { ...ended(), at_ms: -1 }],
    ["fractional ended time", { ...ended(), at_ms: 10.1 }],
    ["invalid ended playback id", { ...ended(), playback_id: "bad id!" }],
    ["non-object payload", null],
  ];

  test("drops malformed event variants", async (t) => {
    for (const [name, event] of malformedEvents) {
      await t.test(name, () => assert.equal(parsePlaybackEvent(event), null));
    }
  });

  test("counts malformed events and never renders them", () => {
    let state = createPlaybackState();
    state = reducePlayback(state, { ...started(), visual_url: "file:///secret.webm" });
    state = reducePlayback(state, { ...started(), playback_id: "x" });
    for (const event_type of ["Health_danger", "a".repeat(81), ""]) {
      state = reducePlayback(state, started({ event_type }));
    }

    assert.equal(state.malformedEventCount, 5);
    assert.equal(state.visual, null);
  });

  test("processes backwards at_ms values in arrival order", () => {
    let state = reducePlayback(createPlaybackState(), started({ at_ms: 1000 }));
    state = reducePlayback(
      state,
      started({ playback_id: "pb_00000002", at_ms: 500, visual_url: "kohbor-media://ins_00000002/video/new.webm" }),
    );
    state = reducePlayback(state, ended("pb_00000001", { at_ms: 2000 }));

    assert.equal(state.currentPlaybackId, "pb_00000002");
    assert.equal(state.visual?.playback_id, "pb_00000002");
    state = reducePlayback(state, ended("pb_00000002", { at_ms: 400 }));
    assert.equal(state.visual, null);
  });

  test("long event sequences retain at most one visual and never queue", () => {
    let state = createPlaybackState();
    for (let index = 0; index < 1000; index += 1) {
      const playbackId = `pb_${String(index).padStart(8, "0")}`;
      state = reducePlayback(
        state,
        started({
          playback_id: playbackId,
          at_ms: 1000 - index,
          visual_url: index % 3 === 0 ? null : `kohbor-media://ins_${String(index).padStart(8, "0")}/video/clip.webm`,
          class: index % 5 === 0 ? "voice" : "ordinary",
        }),
      );
      assert.equal(state.visual === null || !Array.isArray(state.visual), true);
      assert.equal("queue" in state, false);
      if (index % 2 === 0) state = reducePlayback(state, ended(playbackId, { at_ms: 500 - index }));
    }
  });
}
