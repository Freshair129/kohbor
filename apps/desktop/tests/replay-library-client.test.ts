import assert from "node:assert/strict";
import test from "node:test";
import { createReplayLibraryClient } from "../src/replay/replayLibraryClient.ts";

test("replay library client invokes each host command with typed arguments", async () => {
  const calls: Array<{ command: string; args?: Record<string, unknown> }> = [];
  const transport = {
    async invoke<T>(command: string, args?: Record<string, unknown>): Promise<T> {
      calls.push({ command, args });
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
