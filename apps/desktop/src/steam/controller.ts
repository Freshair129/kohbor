// @ts-ignore Explicit extensions support Node's TypeScript loader.
import { deriveLaunchView, parseLaunchResult, parseSteamStatus, type LaunchResult, type SteamStatus } from "./model.ts";

export function connectSteamStatus(port: { get(): Promise<unknown>; subscribe(cb: (value: unknown) => void): Promise<() => void> }, update: (status: SteamStatus | null) => void): () => void {
  let disposed = false; let serial = 0; let unsubscribe: (() => void) | undefined;
  void (async () => {
    try {
      const stop = await port.subscribe(value => { if (!disposed) { serial++; update(parseSteamStatus(value)); } });
      if (disposed) { stop(); return; }
      unsubscribe = stop;
    } catch { /* An older host can still answer the initial command. */ }
    if (disposed) return;
    const beforeRead = serial;
    try {
      const value = await port.get();
      if (!disposed && serial === beforeRead) update(parseSteamStatus(value));
    } catch { if (!disposed && serial === beforeRead) update(null); }
  })();
  return () => { disposed = true; unsubscribe?.(); };
}

export function createDotaLauncher(invoke: (command: string) => Promise<unknown>) {
  let state: { pending: boolean; awaitingStatus: boolean; attempted: boolean; result: LaunchResult | null } = { pending: false, awaitingStatus: false, attempted: false, result: null };
  const listeners = new Set<() => void>();
  const update = (patch: Partial<typeof state>) => { state = { ...state, ...patch }; listeners.forEach(cb => cb()); };
  return {
    getSnapshot: () => state,
    subscribe(cb: () => void) { listeners.add(cb); return () => { listeners.delete(cb); }; },
    // A successful request waits for another real host status, never a synthetic timer.
    observeStatus() { if (state.awaitingStatus) update({ awaitingStatus: false }); },
    async launch(status: SteamStatus | null): Promise<void> {
      if (!deriveLaunchView(status, state.pending || state.awaitingStatus, "th").enabled) return;
      update({ pending: true, attempted: true, result: null });
      try {
        const result = parseLaunchResult(await invoke("launch_dota"));
        update({ result, awaitingStatus: result?.result === "launched" });
      }
      catch { update({ result: null }); }
      finally { update({ pending: false }); }
    },
  };
}
