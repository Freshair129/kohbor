import type { IconKind, MatchBundle } from "./matchBundle";
import type { MapCalibration, ReplayPatchData } from "./replayPack.mjs";
export function mountMatchView(
  root: ShadowRoot,
  bundle: MatchBundle,
  options: {
    locale: "th" | "en";
    markup: string;
    style: string;
    icon: (kind: IconKind, name: string) => string;
    hasMap: boolean;
    patchData: ReplayPatchData;
    mapCalibration: MapCalibration;
    /** element id to text, written into the markup before the viewer starts */
    header: Record<string, string>;
  },
): () => void;
