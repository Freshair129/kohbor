// @ts-ignore Node's built-in TypeScript loader requires the explicit source extension.
import { liveCopy, type LiveLocale } from "./copy.ts";

export function dangerLabel(level: number | null, locale: LiveLocale): string {
  if (level === null) return liveCopy(locale).noRiskData;
  return liveCopy(locale).meterLabels[level] ?? liveCopy(locale).noRiskData;
}

export function dangerTone(level: number | null): "unknown" | "none" | "warn" | "critical" {
  if (level === null) return "unknown";
  if (level === 0) return "none";
  return level >= 3 ? "critical" : "warn";
}

export function litDangerLeds(level: number | null): number[] {
  return level === null ? [] : [0, 1, 2, 3].filter((led) => led <= level);
}
