import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { test } from "node:test";
import { filterLogEntries, formatLogLine, copyLogLines, mergeLogEntries, shouldFollowLogs, parseLogTail, type LogEntry } from "../src/logs/model.ts";
import { createLogTail } from "../src/logs/tail.ts";
import { shapeFrontendError, createFrontendForwarder, installFrontendLogging } from "../src/logs/frontend.ts";
import { DEVELOPER_MODE_DEFAULT, DEVELOPER_MODE_KEY, readDeveloperMode, writeDeveloperMode } from "../src/shell/developerMode.ts";
import { shellTabs, nextShellTab } from "../src/shell/state.ts";

const entry = (seq: number, level: LogEntry["level"] = "info"): LogEntry => ({ seq, ts: `t${seq}`, level,
  module: "capture", event: "frame", msg: `message ${seq}`, fields: { count: seq } });
const reply = (entries: LogEntry[], dropped = false) => ({ entries, last_seq: entries.at(-1)?.seq ?? 0, dropped });

test("minimum level filter includes more severe levels, with all/debug showing every level", () => {
  const entries = [entry(1, "error"), entry(2, "warn"), entry(3), entry(4, "debug")];
  for (const [level, expected] of [["all", 4], ["error", 1], ["warn", 2], ["info", 3], ["debug", 4]] as const) {
    assert.equal(filterLogEntries(entries, level, "").length, expected);
  }
});

test("text filtering is literal and case insensitive across all displayed log fields", () => {
  const entries = [entry(1), { ...entry(2, "error"), msg: "Alpha [error]", fields: { hero: "Lion" } }];
  for (const text of ["ALPHA", "lion", "[error]", '"hero"']) assert.deepEqual(filterLogEntries(entries, "all", text).map(e => e.seq), [2]);
  assert.equal(filterLogEntries(entries, "error", "message").length, 0);
  assert.equal(filterLogEntries(entries, "all", "capture").length, 2);
});

test("copy formats one plain-text line per entry and takes the newest requested filtered lines", () => {
  assert.equal(formatLogLine(entry(1)), 't1 info capture frame message 1 {"count":1}');
  assert.equal(formatLogLine({ ...entry(2), msg: "first\nsecond\r\nthird" }), 't2 info capture frame first\\nsecond\\nthird {"count":2}');
  assert.equal(copyLogLines([entry(1), entry(2), entry(3)], 2), [entry(2), entry(3)].map(formatLogLine).join("\n"));
  assert.equal(copyLogLines([], 50), "");
});

test("log rows merge by sequence oldest first, deduplicate and keep at most the newest 1000", () => {
  const incoming = Array.from({ length: 1100 }, (_, i) => entry(i + 1)).reverse();
  const rows = mergeLogEntries([entry(10)], incoming);
  assert.equal(rows.length, 1000); assert.equal(rows[0]?.seq, 101); assert.equal(rows.at(-1)?.seq, 1100);
  assert.equal(mergeLogEntries([entry(1)], [entry(1), entry(2)]).length, 2);
});

test("follow stops when scrolled up and resumes only at the bottom including rounding tolerance", () => {
  assert.equal(shouldFollowLogs({ scrollTop: 200, scrollHeight: 1000, clientHeight: 300 }), false);
  assert.equal(shouldFollowLogs({ scrollTop: 699, scrollHeight: 1000, clientHeight: 300 }), true);
  assert.equal(shouldFollowLogs({ scrollTop: 0, scrollHeight: 100, clientHeight: 300 }), true);
});

test("tail parser rejects malformed data instead of creating rows or default sequence numbers", () => {
  assert.deepEqual(parseLogTail(reply([entry(1)])), reply([entry(1)]));
  for (const bad of [null, {}, { entries: [], dropped: false }, { ...reply([entry(1)]), last_seq: -1 },
    reply([{ ...entry(1), level: "success" } as never]), reply([{ ...entry(1), fields: null } as never]),
    { ...reply([entry(2)]), last_seq: 1 }]) assert.equal(parseLogTail(bad), null);
});

test("tail polls incrementally, preserves dropped notice and clears only client rows", async () => {
  const calls: unknown[] = []; let count = 0;
  const tail = createLogTail(async (command, args) => { calls.push([command, args]); return ++count === 1 ? reply([entry(1)]) : reply([entry(3)], true); });
  tail.setVisible(true); await tail.poll(); await tail.poll();
  assert.deepEqual(calls, [["log_tail", { limit: 1000 }], ["log_tail", { since_seq: 1, limit: 1000 }]]);
  assert.deepEqual(tail.getSnapshot().entries.map(e => e.seq), [1, 3]);
  assert.equal(tail.getSnapshot().dropped, true);
  tail.clear(); assert.equal(tail.getSnapshot().entries.length, 0);
  assert.equal(tail.getSnapshot().lastSeq, 3); assert.equal(tail.getSnapshot().dropped, false);
  await tail.poll(); assert.deepEqual(calls.at(-1), ["log_tail", { since_seq: 3, limit: 1000 }]);
});

test("missing or malformed host commands resolve to an honest unavailable state without throwing", async () => {
  for (const invoke of [async () => { throw new Error("Command not found"); }, async () => ({})]) {
    const tail = createLogTail(invoke); tail.setVisible(true);
    await tail.poll(); assert.equal(tail.getSnapshot().status, "unavailable");
    assert.deepEqual(tail.getSnapshot().entries, []); assert.equal(tail.getSnapshot().lastSeq, null);
  }
});

test("pause and hidden tab suppress polling and an in-flight reply cannot update a paused view", async () => {
  let calls = 0; let resolve!: (value: unknown) => void;
  const tail = createLogTail(() => { calls++; return new Promise(done => { resolve = done; }); });
  await tail.poll(); assert.equal(calls, 0);
  tail.setVisible(true); const pending = tail.poll(); await tail.poll(); assert.equal(calls, 1);
  tail.setPaused(true); resolve(reply([entry(1)])); await pending;
  assert.equal(tail.getSnapshot().lastSeq, null);
  await tail.poll(); assert.equal(calls, 1);
  tail.setPaused(false); const resumed = tail.poll(); resolve(reply([entry(2)])); await resumed;
  assert.equal(tail.getSnapshot().lastSeq, 2);
  tail.setVisible(false); await tail.poll(); assert.equal(calls, 2);
});

test("clearing during a request advances the cursor without resurrecting cleared rows", async () => {
  let resolve!: (value: unknown) => void;
  const tail = createLogTail(() => new Promise(done => { resolve = done; }));
  tail.setVisible(true); const pending = tail.poll(); tail.clear(); resolve(reply([entry(5)])); await pending;
  assert.deepEqual(tail.getSnapshot().entries, []); assert.equal(tail.getSnapshot().lastSeq, 5);
});

test("host sequence reset replaces the old session instead of mixing duplicate sequence ids", async () => {
  let calls = 0; const tail = createLogTail(async () => reply([entry(++calls === 1 ? 100 : 1)]));
  tail.setVisible(true); await tail.poll(); await tail.poll();
  assert.deepEqual(tail.getSnapshot().entries.map(e => e.seq), [1]); assert.equal(tail.getSnapshot().lastSeq, 1);
  assert.equal(tail.getSnapshot().dropped, false, "a sequence reset is not a host-reported drop");
});

test("developer mode defaults on, persists explicit off and tolerates unavailable storage", () => {
  assert.equal(DEVELOPER_MODE_DEFAULT, true); assert.equal(DEVELOPER_MODE_KEY, "kohbor.developerMode");
  assert.equal(readDeveloperMode(null), true);
  for (const [value, expected] of [[null, true], ["true", true], ["false", false], ["garbage", true]] as const) {
    assert.equal(readDeveloperMode({ getItem: () => value }), expected);
  }
  assert.equal(readDeveloperMode({ getItem: () => { throw new Error("denied"); } }), true);
  let saved: unknown; assert.equal(writeDeveloperMode({ setItem: (...args) => { saved = args; } }, false), true);
  assert.deepEqual(saved, [DEVELOPER_MODE_KEY, "false"]);
  assert.equal(writeDeveloperMode({ setItem: () => { throw new Error("denied"); } }, true), false);
});

test("hidden developer tab is excluded from keyboard navigation", () => {
  assert.deepEqual(shellTabs(false), ["live", "packs", "settings", "status"]);
  assert.equal(shellTabs(true).at(-1), "logs");
  assert.equal(nextShellTab("live", "End", shellTabs(false)), "status");
  assert.equal(nextShellTab("live", "End", shellTabs(true)), "logs");
  assert.equal(nextShellTab("status", "ArrowRight", shellTabs(true)), "logs");
  assert.equal(nextShellTab("logs", "ArrowRight", shellTabs(true)), "live");
});

test("frontend payload keeps only a bounded message and source basename, without object or stack content", () => {
  assert.deepEqual(shapeFrontendError("error", "failed\n at C:\\Users\\private\\file.ts:2", "file:///C:/Users/private/app.js?key=secret"), {
    level: "error", module: "renderer", msg: "failed", fields: { source: "app.js" },
  });
  assert.equal(shapeFrontendError("error", "x".repeat(600)).msg.length, 500);
  assert.deepEqual(shapeFrontendError("unhandledrejection", { secret: "private", toString() { throw new Error(); } }, { path: "secret" }), {
    level: "error", module: "renderer", msg: "Unhandled promise rejection", fields: {},
  });
  assert.doesNotMatch(shapeFrontendError("error", "Cannot open C:\\Users\\private\\file.txt").msg, /Users|private/);
});

test("frontend forwarding allows at most five attempts in a rolling second and swallows host rejection", async () => {
  let now = 0; const calls: unknown[] = [];
  const forward = createFrontendForwarder(async (command, args) => { calls.push([command, args]); throw new Error("missing"); }, () => now);
  for (let i = 0; i < 9; i++) await forward("error", "boom");
  assert.equal(calls.length, 5);
  now = 999; await forward("error", "boom"); assert.equal(calls.length, 5);
  now = 1000; await forward("unhandledrejection", "promise"); assert.equal(calls.length, 6);
  assert.equal((calls[0] as unknown[])[0], "log_frontend");
});

test("frontend listeners install once and extract an Error message without its stack", async () => {
  const target = new EventTarget(); const payloads: unknown[] = [];
  const invoke = async (_command: string, args?: Record<string, unknown>) => { payloads.push(args); };
  installFrontendLogging(target, invoke); installFrontendLogging(target, invoke);
  const event = new Event("unhandledrejection"); Object.defineProperty(event, "reason", { value: new Error("rejected") });
  target.dispatchEvent(event); await Promise.resolve();
  assert.equal(payloads.length, 1); assert.equal((payloads[0] as { msg: string }).msg, "rejected");
  assert.doesNotMatch(JSON.stringify(payloads), /stack|logs\.test/);
});

test("logs layout has a shrinking bounded list as its only scrolling region", () => {
  const css = readFileSync(new URL("../src/logs/logs.css", import.meta.url), "utf8").replace(/\/\*[\s\S]*?\*\//g, "");
  const root = css.match(/\.logs-tab\s*\{([^}]+)\}/)?.[1] ?? "";
  assert.match(root, /height:\s*100%/); assert.match(root, /min-height:\s*0/); assert.match(root, /overflow:\s*hidden/);
  assert.match(root, /grid-template-rows:\s*auto auto minmax\(0,\s*1fr\)/);
  const scrolling = [...css.matchAll(/([^{}]+)\{([^{}]*)\}/g)].filter(match => /overflow(?:-y)?:\s*(?:auto|scroll)/.test(match[2]!));
  assert.deepEqual(scrolling.map(match => match[1]!.trim()), [".logs-list"]);
  assert.match(css, /\.logs-line\s*\{[^}]*overflow-wrap:\s*anywhere[^}]*white-space:\s*pre-wrap/);
});

test("frontend forwarding starts only in the control-window path and the missing host state is explicit", () => {
  const main = readFileSync(new URL("../src/main.tsx", import.meta.url), "utf8");
  assert.match(main, /if\s*\(!overlayView\)\s*installFrontendLogging\(/);
  const messages = readFileSync(new URL("../src/logs/copy.ts", import.meta.url), "utf8");
  assert.match(messages, /ยังไม่มีบันทึกจากโฮสต์/); assert.match(messages, /บางบรรทัดถูกข้าม/);
});
