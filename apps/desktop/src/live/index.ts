import { liveCollection } from "./collection";
import { subscribeLiveEvent } from "./events";

export function startLiveCollection(): Promise<void> {
  return liveCollection.start(subscribeLiveEvent);
}

export { DangerMeter } from "./DangerMeter";
export { EnemySeen } from "./EnemySeen";
export { LiveTab } from "./LiveTab";
export { MinimapPanel } from "./MinimapPanel";
export { ResourcePanel } from "./ResourcePanel";
export { RiskStrip } from "./RiskStrip";
export { useCaptureStatus } from "./useCaptureStatus";
export { useMinimap } from "./useMinimap";
export { useResources } from "./useResources";
export { useRisk } from "./useRisk";
