import { useSyncExternalStore } from "react";
import { liveCollection } from "./collection";
import { useFreshValue } from "./freshness";
import type { CaptureStatus } from "./types";

export function useCaptureStatus(): CaptureStatus | null {
  const snapshot = useSyncExternalStore(
    liveCollection.subscribe,
    liveCollection.getSnapshot,
    liveCollection.getSnapshot,
  );
  return useFreshValue(snapshot.capture);
}
