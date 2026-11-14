// @ts-ignore Explicit extensions support Node's TypeScript loader.
import { LOG_ROW_LIMIT, mergeLogEntries, parseLogTail, type LogEntry } from "./model.ts";

export type LogInvoke = (command: string, args?: Record<string, unknown>) => Promise<unknown>;
export interface LogTailState {
  entries: LogEntry[];
  lastSeq: number | null;
  status: "loading" | "ready" | "unavailable";
  dropped: boolean;
  paused: boolean;
}

/** The hook owns the one-second clock; this controller owns cursor, pause and request races. */
export function createLogTail(invoke: LogInvoke) {
  let state: LogTailState = { entries: [], lastSeq: null, status: "loading", dropped: false, paused: false };
  let visible = false;
  let pending = false;
  let epoch = 0;
  let clearVersion = 0;
  const listeners = new Set<() => void>();
  const update = (patch: Partial<LogTailState>) => {
    state = { ...state, ...patch };
    listeners.forEach(listener => listener());
  };
  return {
    getSnapshot: () => state,
    subscribe(listener: () => void) { listeners.add(listener); return () => { listeners.delete(listener); }; },
    setVisible(value: boolean) { if (visible !== value) { visible = value; epoch++; } },
    setPaused(value: boolean) { if (state.paused !== value) { epoch++; update({ paused: value }); } },
    clear() { clearVersion++; update({ entries: [], dropped: false }); },
    async poll(): Promise<void> {
      if (!visible || state.paused || pending) return;
      pending = true;
      const requestEpoch = epoch;
      const requestClear = clearVersion;
      const current = () => requestEpoch === epoch && visible && !state.paused;
      try {
        const response = parseLogTail(await invoke("log_tail", {
          ...(state.lastSeq === null ? {} : { since_seq: state.lastSeq }), limit: LOG_ROW_LIMIT,
        }));
        if (!current()) return;
        if (!response) { update({ status: "unavailable" }); return; }
        // A restarted host owns a new sequence space; do not blend two sessions.
        const reset = state.lastSeq !== null && response.last_seq < state.lastSeq;
        const cleared = requestClear !== clearVersion;
        update({ entries: cleared ? [] : mergeLogEntries(reset ? [] : state.entries, response.entries),
          lastSeq: response.last_seq, status: "ready", dropped: cleared ? false : (!reset && state.dropped) || response.dropped });
      } catch {
        if (current()) update({ status: "unavailable" });
      } finally { pending = false; }
    },
  };
}
