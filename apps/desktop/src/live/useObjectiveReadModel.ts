import { invoke } from "@tauri-apps/api/core";
import { useEffect, useState } from "react";
import { parseObjectiveReadModel, type ObjectiveReadModel } from "./objectiveCountdownModel";

export interface ReceivedObjectiveReadModel {
  model: ObjectiveReadModel;
  receivedAtMs: number;
}

/** Polls only while the pinned deck is visible; values remain tied to admitted GSI samples. */
export function useObjectiveReadModel(enabled: boolean): ReceivedObjectiveReadModel | null {
  const [received, setReceived] = useState<ReceivedObjectiveReadModel | null>(null);

  useEffect(() => {
    if (!enabled || typeof window === "undefined" || !("__TAURI_INTERNALS__" in window)) {
      setReceived(null);
      return;
    }

    let active = true;
    let inFlight = false;
    const poll = async () => {
      if (!active || document.visibilityState !== "visible" || inFlight) return;
      inFlight = true;
      try {
        const model = parseObjectiveReadModel(await invoke<unknown>("objective_read_model"));
        if (active && document.visibilityState === "visible") {
          setReceived(model ? { model, receivedAtMs: Date.now() } : null);
        }
      } catch {
        if (active) setReceived(null);
      } finally {
        inFlight = false;
      }
    };
    const onVisibility = () => {
      if (document.visibilityState === "visible") void poll();
      else setReceived(null);
    };

    void poll();
    const timer = window.setInterval(() => void poll(), 1_000);
    document.addEventListener("visibilitychange", onVisibility);
    return () => {
      active = false;
      window.clearInterval(timer);
      document.removeEventListener("visibilitychange", onVisibility);
    };
  }, [enabled]);

  return received;
}
