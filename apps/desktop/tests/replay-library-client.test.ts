import assert from "node:assert/strict";
import test from "node:test";
import { createReplayLibraryClient } from "../src/replay/replayLibraryClient.ts";

test("replay library client invokes each host command with typed arguments", async () => {
  const calls: Array<{ command: string; args?: Record<string, unknown> }> = [];
  const transport = {
    async invoke<T>(command: string, args?: Record<string, unknown>): Promise<T> {
      calls.push({ command, args });
      if (command === "replay_library_list") return { matches: [], jobs: [] } as T;
      return undefined as T;
    },
    async listen<T>(): Promise<() => void> {
      return () => {};
    },
  };
  const client = createReplayLibraryClient(transport);

  await client.list();
  await client.pickDemFile();
  await client.open("123456");
  await client.parse("C:\\replays\\picked.dem");
  await client.reparse("123456");
  await client.remove("123456");
  await client.clear();

  assert.deepEqual(calls, [
    { command: "replay_library_list", args: undefined },
    { command: "replay_library_pick_file", args: undefined },
    { command: "replay_library_open", args: { matchId: "123456" } },
    {
      command: "replay_library_parse",
      args: { idOrPath: "C:\\replays\\picked.dem" },
    },
    { command: "replay_library_reparse", args: { matchId: "123456" } },
    { command: "replay_library_remove", args: { matchId: "123456" } },
    { command: "replay_library_clear", args: undefined },
  ]);
});

test("state change subscription listens to the fixed replay event", async () => {
  let listenedEvent: string | undefined;
  let receivedPayload: unknown;
  const transport = {
    async invoke<T>(): Promise<T> {
      return undefined as T;
    },
    async listen<T>(
      event: string,
      handler: (payload: T) => void,
    ): Promise<() => void> {
      listenedEvent = event;
      handler({ changed: true } as T);
      return () => {};
    },
  };
  const client = createReplayLibraryClient(transport);

  await client.onStateChanged((payload) => {
    receivedPayload = payload;
  });

  assert.equal(listenedEvent, "replay-library://state-changed");
  assert.deepEqual(receivedPayload, { changed: true });
});

test("client preserves the local key, keyed history reference, and validated bundle hash", async () => {
  const calls: Array<{ command: string; args?: Record<string, unknown> }> = [];
  const wireEntry = {
    match_id: "424243",
    match_key: "opaque-match-key",
    source: "dota_replays_folder",
    engine_version: "0.3.0",
    parse_time_unix_ms: null,
    status: { state: "ready" },
    bundle_bytes: 128,
    storage_bytes: 256,
    created_at_unix_ms: 1790000000000,
    match_date_utc: null,
    own_hero: null,
    duration_s: null,
    patch: null,
    source_exists: true,
    bundle_schema: "3.1",
    bundle_hash: "a".repeat(64),
  };
  const transport = {
    async invoke<T>(command: string, args?: Record<string, unknown>): Promise<T> {
      calls.push({ command, args });
      if (command === "replay_library_list") return { matches: [wireEntry], jobs: [] } as T;
      if (command === "replay_library_watcher_state") return {
        folder_found: true, last_scan_unix_ms: 1790000000000, last_error_code: null,
      } as T;
      if (command === "replay_library_retention_preview") return { evicted_matches: 2 } as T;
      return undefined as T;
    },
    async listen<T>(): Promise<() => void> { return () => {}; },
  };
  const client = createReplayLibraryClient(transport);

  const list = await client.list();
  assert.equal(list.matches[0]?.match_id, "424243");
  assert.equal(list.matches[0]?.match_key, "opaque-match-key");
  assert.equal(list.matches[0]?.source_exists, true);
  assert.equal(list.matches[0]?.bundle_schema, "3.1");
  assert.equal(list.matches[0]?.bundle_hash, "a".repeat(64));
  assert.equal(list.watched_replay_folder_path, null);
  for (const key of ["source_path", "dem_sha256", "engine_sha256"]) {
    assert.equal(key in (list.matches[0] ?? {}), false, `${key} must not be exposed`);
  }

  assert.deepEqual(await client.watcherState(), {
    folder_found: true, last_scan_unix_ms: 1790000000000, last_error_code: null,
  });
  assert.deepEqual(await client.retentionPreview(10, 2 * 1024 ** 3), { evicted_matches: 2 });
  await client.retentionSave(10, 2 * 1024 ** 3, 2);
  await client.pin("424243", true);
  assert.deepEqual(calls.slice(0, 5), [
    { command: "replay_library_list", args: undefined },
    { command: "replay_library_watcher_state", args: undefined },
    { command: "replay_library_retention_preview", args: { keepNewest: 10, maxBytes: 2 * 1024 ** 3 } },
    { command: "replay_library_retention_save", args: { keepNewest: 10, maxBytes: 2 * 1024 ** 3, expectedEvictions: 2 } },
    { command: "replay_library_pin", args: { matchId: "424243", pinned: true } },
  ]);
});
