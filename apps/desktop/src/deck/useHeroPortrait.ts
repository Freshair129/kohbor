import { invoke } from "@tauri-apps/api/core";
import { useEffect, useState } from "react";
import { createHeroPortraitCache, type HeroPortraitResult } from "./heroPortraitModel";

const portraitCache = createHeroPortraitCache((command, args) => invoke<unknown>(command, args));

export function useHeroPortrait(heroId: string | null): HeroPortraitResult {
  const [loaded, setLoaded] = useState(() => ({ heroId, result: portraitCache.peek(heroId) ?? { state: "loading" as const } }));
  useEffect(() => {
    let disposed = false;
    void portraitCache.load(heroId).then(result => { if (!disposed) setLoaded({ heroId, result }); });
    return () => { disposed = true; };
  }, [heroId]);
  // Never flash the previous hero's image while a new id is loading.
  return loaded.heroId === heroId ? loaded.result : portraitCache.peek(heroId) ?? { state: "loading" };
}
