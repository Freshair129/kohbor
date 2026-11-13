import { useSyncExternalStore } from "react";
import { liveCollection } from "./collection";
import { useFreshValue } from "./freshness";
import type { ResourceMetrics, ResourceSummary } from "./types";

export interface ResourcePanelState {
  current: ResourceMetrics | null;
  summary: ResourceSummary;
  reset: () => void;
}

export function useResources(): ResourcePanelState {
  const snapshot = useSyncExternalStore(
    liveCollection.subscribe,
    liveCollection.getSnapshot,
    liveCollection.getSnapshot,
  );
  const current = useFreshValue(snapshot.resources);
  return {
    current,
    summary: snapshot.resourceSummary,
    reset: liveCollection.resetResourceSummary,
  };
}
