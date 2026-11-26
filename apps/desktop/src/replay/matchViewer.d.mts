import type { IconKind, MatchBundle } from "./matchBundle";
import type { MapCalibration, ReplayPatchData } from "./replayPack.mjs";
import type { StatIconDefinition, StatIconId, StatKey } from "../live/statIcons/catalog";
import type { HeroCardDisplay } from "../shared/heroCardDisplay.mjs";
export function mountMatchView(
  root: ShadowRoot,
  bundle: MatchBundle,
  options: {
    locale: "th" | "en";
    markup: string;
    style: string;
    icon: (kind: IconKind, name: string) => string;
    statCatalog: Readonly<Record<StatKey, StatIconDefinition>>;
    statIcon: (id: StatIconId) => string | null;
    hasMap: boolean;
    hasIcon?: (kind: "ability" | "item", name: string) => boolean;
    patchData: ReplayPatchData;
    mapCalibration: MapCalibration;
    /** element id to text, written into the markup before the viewer starts */
    header: Record<string, string>;
    /** seconds to open at; clamped to the match range */
    initialTime?: number | null;
    disableKeyboard?: boolean;
    statLabelMode?: "text" | "icon";
    heroCardDisplay?: HeroCardDisplay | null;
    density?: "full" | "compact" | "roster";
    selectedHeroId?: string | null;
    cardsCollapsed?: boolean | { radiant: boolean; dire: boolean };
  },
): () => void;
