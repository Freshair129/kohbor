import { useState } from "react";
import { deriveHeroPortraitView } from "../deck/heroPortraitModel";
import { useHeroPortrait } from "../deck/useHeroPortrait";

/**
 * Upright hero portrait for a draft card (card v3, owner Variant B, 2026-11-23). The host decodes it
 * from the installed game files through the existing `hero_portrait` command, read-only, in memory;
 * nothing is bundled or written to disk. Missing, loading or failed portraits show a neutral "?".
 */
export function OverlayPortrait({ heroKey }: { heroKey: string | null }) {
  const result = useHeroPortrait(heroKey);
  const [failed, setFailed] = useState<{ heroKey: string | null; src: string } | null>(null);
  const view = deriveHeroPortraitView(heroKey, result, failed !== null && failed.heroKey === heroKey && failed.src === result.src);
  return (
    <span className={`draft-ov-pt${view.src ? "" : " is-missing"}`} aria-hidden="true">
      {view.src
        ? <img src={view.src} alt="" decoding="async" draggable={false} onError={() => setFailed({ heroKey, src: view.src! })} />
        : "?"}
    </span>
  );
}
