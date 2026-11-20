import { useEffect, useState } from "react";

export const GLASS_LEVELS = ["off", "low", "medium", "high"] as const;
export type GlassLevel = (typeof GLASS_LEVELS)[number];
export const DEFAULT_GLASS_LEVEL: GlassLevel = "medium";
const GLASS_STORAGE_KEY = "kohbor.glass-level";

export function isGlassLevel(value: unknown): value is GlassLevel {
  return typeof value === "string" && GLASS_LEVELS.some(level => level === value);
}

export function readGlassLevel(storage: Pick<Storage, "getItem">): GlassLevel {
  try {
    const saved = storage.getItem(GLASS_STORAGE_KEY);
    return isGlassLevel(saved) ? saved : DEFAULT_GLASS_LEVEL;
  } catch {
    return DEFAULT_GLASS_LEVEL;
  }
}

export function writeGlassLevel(storage: Pick<Storage, "setItem">, value: GlassLevel): void {
  try { storage.setItem(GLASS_STORAGE_KEY, value); } catch { /* In-memory display still changes. */ }
}

export function solidGlassFallback(level: GlassLevel, reducedTransparency: boolean, forcedColors: boolean): boolean {
  return level === "off" || reducedTransparency || forcedColors;
}

export function useGlassPreference() {
  const [level, setLevel] = useState<GlassLevel>(() => {
    if (typeof window === "undefined") return DEFAULT_GLASS_LEVEL;
    try { return readGlassLevel(window.localStorage); } catch { return DEFAULT_GLASS_LEVEL; }
  });
  useEffect(() => {
    document.documentElement.dataset.glass = level;
  }, [level]);
  return {
    level,
    changeLevel(value: GlassLevel) {
      if (!isGlassLevel(value)) return;
      setLevel(value);
      try { writeGlassLevel(window.localStorage, value); } catch { /* In-memory display still changes. */ }
    },
  };
}
