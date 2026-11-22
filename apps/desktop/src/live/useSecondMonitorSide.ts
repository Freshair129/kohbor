import { useCallback, useState } from "react";
import {
  DEFAULT_SECOND_MONITOR_SIDE,
  isSecondMonitorSide,
  persistSecondMonitorSide,
  readSecondMonitorSide,
  type SecondMonitorSide,
} from "./secondMonitorModel";

/** Per-device display preference (localStorage), like the rail state. Default off; the user picks the side once. */
export function useSecondMonitorSide() {
  const [side, setSide] = useState<SecondMonitorSide>(() => {
    if (typeof window === "undefined") return DEFAULT_SECOND_MONITOR_SIDE;
    try { return readSecondMonitorSide(window.localStorage); } catch { return DEFAULT_SECOND_MONITOR_SIDE; }
  });
  const change = useCallback((next: SecondMonitorSide) => {
    if (!isSecondMonitorSide(next)) return;
    setSide(next);
    if (typeof window !== "undefined") {
      try { persistSecondMonitorSide(window.localStorage, next); } catch { /* In-memory display still changes. */ }
    }
  }, []);
  return { side, change };
}
