import { useEffect, useState, useSyncExternalStore } from "react";
import { usePageVisibility } from "../live/usePageVisibility";
import { logHostInvoke } from "./host";
import { createLogTail } from "./tail";

export function useLogs(visible: boolean) {
  const [tail] = useState(() => createLogTail(logHostInvoke));
  const state = useSyncExternalStore(tail.subscribe, tail.getSnapshot, tail.getSnapshot);
  const visibility = usePageVisibility();
  const active = visible && visibility === "visible";
  useEffect(() => {
    tail.setVisible(active);
    if (active && !state.paused) {
      void tail.poll();
      const timer = window.setInterval(() => { void tail.poll(); }, 1000);
      return () => { window.clearInterval(timer); tail.setVisible(false); };
    }
    return () => tail.setVisible(false);
  }, [tail, active, state.paused]);
  return { state, tail };
}
