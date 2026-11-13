import { useEffect, useState } from "react";
import { subscribeLiveEvent } from "./events";
import { useFreshValue, type TimedValue } from "./freshness";
import { expireVisionEvent, parseRiskState, VISION_EVENT_MS } from "./state";
import type { RiskState, RiskView, VisionEvent } from "./types";

interface TimedVisionEvent {
  event: VisionEvent;
  receivedAt: number;
}

export function useRisk(): RiskView | null {
  const [timed, setTimed] = useState<TimedValue<RiskState>>({ value: null, receivedAt: null });
  const [lastVisionEvent, setLastVisionEvent] = useState<TimedVisionEvent | null>(null);
  const [, setVisionEventTick] = useState(0);
  const state = useFreshValue(timed);
  const freshEvent = expireVisionEvent(lastVisionEvent?.event ?? null, lastVisionEvent?.receivedAt ?? null, performance.now());

  useEffect(() => {
    if (!lastVisionEvent) return;
    const remaining = Math.max(0, VISION_EVENT_MS + 1 - (performance.now() - lastVisionEvent.receivedAt));
    const timer = window.setTimeout(() => setVisionEventTick((current) => current + 1), remaining);
    return () => window.clearTimeout(timer);
  }, [lastVisionEvent?.receivedAt]);

  useEffect(() => {
    let disposed = false;
    let unlisten: (() => void) | undefined;
    void subscribeLiveEvent("risk", (payload) => {
      if (disposed) return;
      const parsed = parseRiskState(payload);
      const receivedAt = performance.now();
      setTimed({ value: parsed, receivedAt });
      const event = parsed?.events.filter((candidate): candidate is VisionEvent => candidate !== "risk_unavailable").at(-1) ?? null;
      if (event) setLastVisionEvent({ event, receivedAt });
    }).then((stop) => {
      if (disposed) stop();
      else unlisten = stop;
    }).catch(() => {
      // A failed subscription leaves risk and enemy visibility unknown.
    });
    return () => {
      disposed = true;
      unlisten?.();
    };
  }, []);

  return state || freshEvent ? { state, lastVisionEvent: freshEvent } : null;
}
