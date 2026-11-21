import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";
import { parseDraftColumnView, retryBudgetView } from "../src/draft/draftColumnModel.ts";
import { startDraftViewPolling, type DraftViewPollEnvironment } from "../src/draft/draftViewPoller.ts";

function payload(used: number) {
  return {
    schema: 1,
    mode: "suggestions",
    overlay: { visible: true, hidden_reason: null },
    phase: "hero_selection",
    placement_verified: true,
    data_state: "live",
    context: { position: 1, bracket: 3 },
    source: {
      provider: "OpenDota", window: null, fetched_at_ms: 1790000000000, failed_at_ms: null,
      failure: null, http_status: null, retry_after_ms: null, table_requests_24h: used, table_request_budget: 4,
    },
    suggestions: {
      rows: [], ranked_count: 0,
      draft_bar: { available: false, identified: 0, slots: 10 },
    },
    pick_card: null,
  };
}

test("Settings poll updates a spent retry budget without a settings action", async () => {
  const settingsSource = readFileSync(new URL("../src/draft/DraftSettings.tsx", import.meta.url), "utf8");
  assert.match(settingsSource, /startDraftViewPolling\(/);
  assert.match(settingsSource, /retryBudget\.used === null \|\| retryBudget\.spent/);
  assert.match(settingsSource, /await draftColumnApi\.retry\(\)/, "the host command remains the authority for retry");
  assert.match(settingsSource, /viewRequests\.isCurrent\(request\)/, "manual refreshes share the host-view sequence gate");

  let visible = false;
  let used = 3;
  let requests = 0;
  let latest: ReturnType<typeof parseDraftColumnView> | null = null;
  let intervalTick: (() => void) | null = null;
  const visibilityListeners = new Set<() => void>();
  const environment: DraftViewPollEnvironment = {
    isVisible: () => visible,
    subscribeVisibility: (listener) => {
      visibilityListeners.add(listener);
      return () => { visibilityListeners.delete(listener); };
    },
    setInterval: (callback, intervalMs) => {
      assert.equal(intervalMs, 2000);
      intervalTick = callback;
      return 1;
    },
    clearInterval: () => { intervalTick = null; },
  };

  const stop = startDraftViewPolling(
    async () => {
      requests += 1;
      return parseDraftColumnView(payload(used));
    },
    (view) => { latest = view; },
    (error) => { assert.fail(`unexpected poll error: ${String(error)}`); },
    environment,
  );

  assert.equal(requests, 0, "hidden Settings does not poll");
  visible = true;
  for (const listener of visibilityListeners) listener();
  await new Promise((resolve) => setTimeout(resolve, 0));
  assert.equal(requests, 1);
  assert.equal(retryBudgetView(latest).spent, false);

  used = 4;
  assert.ok(intervalTick, "visible Settings starts the 2 s poll timer");
  intervalTick();
  await new Promise((resolve) => setTimeout(resolve, 0));
  assert.equal(requests, 2);
  assert.equal(retryBudgetView(latest).label, "4/4 ใน 24 ชม.");
  assert.equal(retryBudgetView(latest).spent, true, "fresh host state disables retry without user interaction");

  visible = false;
  for (const listener of visibilityListeners) listener();
  assert.equal(intervalTick, null, "hidden Settings stops its timer");
  stop();
});

test("Settings polling applies only the newest overlapping host response", async () => {
  let visible = true;
  let intervalTick: (() => void) | null = null;
  const visibilityListeners = new Set<() => void>();
  const responses: Array<(view: ReturnType<typeof parseDraftColumnView>) => void> = [];
  const applied: number[] = [];
  const environment: DraftViewPollEnvironment = {
    isVisible: () => visible,
    subscribeVisibility: (listener) => { visibilityListeners.add(listener); return () => { visibilityListeners.delete(listener); }; },
    setInterval: (callback, intervalMs) => { assert.equal(intervalMs, 2000); intervalTick = callback; return 1; },
    clearInterval: () => { intervalTick = null; },
  };
  const stop = startDraftViewPolling(
    () => new Promise((resolve) => { responses.push(resolve); }),
    (view) => { applied.push(retryBudgetView(view).used ?? -1); },
    (error) => { assert.fail(`unexpected poll error: ${String(error)}`); },
    environment,
  );

  assert.equal(responses.length, 1, "visible Settings starts the initial host request");
  intervalTick?.();
  assert.equal(responses.length, 2, "a later poll may overlap a slow host response");

  responses[1]!(parseDraftColumnView(payload(4)));
  await new Promise((resolve) => setTimeout(resolve, 0));
  responses[0]!(parseDraftColumnView(payload(3)));
  await new Promise((resolve) => setTimeout(resolve, 0));

  assert.deepEqual(applied, [4], "the older response cannot roll back the host's newest budget");
  stop();
});
