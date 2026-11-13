import { useEffect, useState } from "react";
import { isFresh, UNKNOWN_AFTER_MS } from "./state";

export interface TimedValue<T> {
  value: T | null;
  receivedAt: number | null;
}

export function useFreshValue<T>(timed: TimedValue<T>, timeoutMs = UNKNOWN_AFTER_MS): T | null {
  const [now, setNow] = useState(() => performance.now());

  useEffect(() => {
    if (timed.value === null || timed.receivedAt === null) return;
    const remaining = Math.max(0, timeoutMs + 1 - (performance.now() - timed.receivedAt));
    const timer = window.setTimeout(() => setNow(performance.now()), remaining);
    return () => window.clearTimeout(timer);
  }, [timed.receivedAt, timed.value, timeoutMs]);

  return isFresh(timed.value, timed.receivedAt, now, timeoutMs) ? timed.value : null;
}
