import assert from "node:assert/strict";
import { test } from "node:test";

const probe = await import("../src/probe.ts");
const install_id = "ins_0000000001";
const path_sha256 = "b".repeat(64);
const item = {
  path_sha256,
  url: `kohbor-media://localhost/v1/${install_id}/${path_sha256}`,
  duration_ms: 1_200,
  visual_tail_ms: 300,
};
const request = { install_id, revision: 7, items: [item] };

test("strictly parses a valid media probe request", () => {
  assert.deepEqual(probe.parseProbeRequest(request), request);
});

test("rejects malformed request envelopes", () => {
  assert.equal(probe.parseProbeRequest({ ...request, extra: true }), null);
  assert.equal(probe.parseProbeRequest({ ...request, install_id: "bad" }), null);
  assert.equal(probe.parseProbeRequest({ ...request, revision: 0 }), null);
  assert.equal(probe.parseProbeRequest({ ...request, items: "not-an-array" }), null);
});

test("rejects malformed items while retaining valid siblings", () => {
  const parsed = probe.parseProbeRequest({
    ...request,
    items: [
      item,
      { ...item, path_sha256: "B".repeat(64) },
      { ...item, url: `${item.url}?query=1` },
      { ...item, url: item.url.replace(path_sha256, "c".repeat(64)) },
      { ...item, duration_ms: 1.5 },
      { ...item, visual_tail_ms: -1 },
      { ...item, extra: "rejected" },
    ],
  });

  assert.deepEqual(parsed?.items, [item]);
});

test("groups strictly validated pending items by install and revision", () => {
  const pendingItem = { ...item, install_id, revision: request.revision };
  assert.deepEqual(probe.parsePendingProbeRequests([pendingItem]), [request]);
  assert.deepEqual(probe.parsePendingProbeRequests([]), []);
  assert.deepEqual(probe.parsePendingProbeRequests({ ...request, extra: true }), []);
});

test("builds success and failure reports with integer milliseconds", () => {
  const context = { install_id, revision: 7, path_sha256 };
  assert.deepEqual(probe.buildProbeReport(context, true, 1_234), { ...context, ok: true, duration_ms: 1_234 });
  assert.deepEqual(probe.buildProbeReport(context, false, 1_234), { ...context, ok: false, duration_ms: 0 });
  assert.deepEqual(probe.buildProbeReport(context, true, 1.5), { ...context, ok: false, duration_ms: 0 });
});

test("converts only finite positive video durations to integer milliseconds", () => {
  assert.equal(probe.durationSecondsToMilliseconds(1.2345), 1_235);
  assert.equal(probe.durationSecondsToMilliseconds(Number.NaN), null);
  assert.equal(probe.durationSecondsToMilliseconds(Number.POSITIVE_INFINITY), null);
  assert.equal(probe.durationSecondsToMilliseconds(0), null);
});

test("times out decode work using the injected clock", async () => {
  let callback: (() => void) | undefined;
  let delay = 0;
  let cleared = false;
  const clock = {
    setTimeout(next: () => void, delay_ms: number) {
      callback = next;
      delay = delay_ms;
      return 1;
    },
    clearTimeout(handle: unknown) {
      assert.equal(handle, 1);
      cleared = true;
    },
  };
  const work = new Promise<void>(() => {});
  const timed = probe.withProbeTimeout(work, clock);

  assert.equal(delay, 5_000);
  callback?.();
  assert.deepEqual(await timed, { status: "timeout" });
  assert.equal(cleared, true);
});

test("clears the injected timeout after decode resolves", async () => {
  let cleared = false;
  const clock = {
    setTimeout() { return 9; },
    clearTimeout(handle: unknown) {
      assert.equal(handle, 9);
      cleared = true;
    },
  };

  assert.deepEqual(await probe.withProbeTimeout(Promise.resolve("decoded"), clock), {
    status: "resolved",
    value: "decoded",
  });
  assert.equal(cleared, true);
});

test("limits probe work to two concurrent items", async () => {
  let active = 0;
  let peak = 0;
  let completed = 0;
  const release: Array<() => void> = [];
  const enqueue = probe.createProbeQueue<number>(2, async () => {
    active += 1;
    peak = Math.max(peak, active);
    await new Promise<void>((resolve) => release.push(resolve));
    active -= 1;
    completed += 1;
  });
  enqueue([1, 2, 3, 4, 5]);

  while (completed < 5) {
    await new Promise<void>((resolve) => setImmediate(resolve));
    for (const finish of release.splice(0)) finish();
  }

  assert.equal(peak, 2);
  assert.equal(active, 0);
});
