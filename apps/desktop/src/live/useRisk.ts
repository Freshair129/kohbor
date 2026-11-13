import { useSyncExternalStore } from "react";
import { liveCollection } from "./collection";
import { useFreshValue } from "./freshness";
import { captureGatedRiskView, VISION_EVENT_MS } from "./state";
import type { RiskView } from "./types";

export function useRisk(): RiskView | null {
  const snapshot = useSyncExternalStore(
    liveCollection.subscribe,
    liveCollection.getSnapshot,
    liveCollection.getSnapshot,
  );
  const capture = useFreshValue(snapshot.capture);
  const state = useFreshValue(snapshot.risk);
  const lastVisionEvent = useFreshValue(snapshot.lastVisionEvent, VISION_EVENT_MS);
  const view = captureGatedRiskView(state, lastVisionEvent, capture);
  return view.state || view.lastVisionEvent ? view : null;
}
