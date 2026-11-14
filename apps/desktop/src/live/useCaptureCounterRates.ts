import { useSyncExternalStore } from "react";
import { liveCollection } from "./collection";
import { useFreshValue } from "./freshness";

export function useCaptureCounterRates() {
  const snapshot = useSyncExternalStore(liveCollection.subscribe, liveCollection.getSnapshot, liveCollection.getSnapshot);
  return useFreshValue(snapshot.captureCounterRates);
}
