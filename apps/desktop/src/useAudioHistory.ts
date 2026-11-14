import { useCallback, useEffect, useRef, useState } from "react";
import { readAudioHistory, type AudioHistory, type HistoryLoadState } from "./history";
import { audioHistory } from "./runtime";

/** Shared by the pack-history screen and the Deck; inactive tabs unmount their reader. */
export function useAudioHistory(disabled = false) {
  const [history, setHistory] = useState<AudioHistory | null>(null);
  const [state, setState] = useState<HistoryLoadState>("loading");
  const [errorCode, setErrorCode] = useState<string | null>(null);
  const alive = useRef(false);
  const pending = useRef<Promise<void> | null>(null);

  const load = useCallback((): Promise<void> => {
    if (pending.current) return pending.current;
    const work = (async () => {
      try {
        const parsed = await readAudioHistory(audioHistory);
        if (!alive.current) return;
        setHistory(parsed);
        setState(parsed ? "ok" : "unreadable");
        setErrorCode(null);
      } catch (error) {
        if (!alive.current) return;
        // Keep previous records for the detailed history screen, visibly outdated there.
        setState("failed");
        setErrorCode(typeof error === "string" ? error : error instanceof Error ? error.message : "UNKNOWN");
      }
    })();
    pending.current = work;
    void work.finally(() => { if (pending.current === work) pending.current = null; });
    return work;
  }, []);

  useEffect(() => {
    alive.current = true;
    return () => { alive.current = false; };
  }, []);

  useEffect(() => {
    if (disabled) return;
    let cancelled = false;
    let running = false;
    let timer: number | undefined;
    const tick = async () => {
      if (cancelled || running || document.hidden) return;
      running = true;
      window.clearTimeout(timer);
      await load();
      running = false;
      if (!cancelled && !document.hidden) timer = window.setTimeout(() => void tick(), 2_000);
    };
    const onVisibility = () => {
      window.clearTimeout(timer);
      if (!document.hidden) void tick();
    };
    document.addEventListener("visibilitychange", onVisibility);
    void tick();
    return () => {
      cancelled = true;
      window.clearTimeout(timer);
      document.removeEventListener("visibilitychange", onVisibility);
    };
  }, [disabled, load]);
  return { history, state, errorCode, load };
}
