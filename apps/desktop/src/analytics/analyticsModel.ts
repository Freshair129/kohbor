import type { ReplayLibraryEntry } from "../replay/replayLibraryClient";

export type AnalyticsViewState = "empty" | "parsing" | "ready" | "aidoc" | "noai" | "list" | "settings";

export function analyticsViewState(input: {
  entries: readonly ReplayLibraryEntry[];
  selectedId: string | null;
  selectedReady: boolean;
  openDocument: boolean;
  aiAvailable: boolean;
  showList: boolean;
  showSettings: boolean;
}): AnalyticsViewState {
  if (input.showSettings) return "settings";
  if (input.showList) return "list";
  if (!input.selectedId && input.entries.length === 0) return "empty";
  if (input.selectedId && !input.selectedReady) return "parsing";
  if (input.selectedId && input.openDocument) return "aidoc";
  if (input.selectedId && !input.aiAvailable) return "noai";
  return "ready";
}

export function analyticsLayout(width: number, height: number, leftCollapsed = false, rightCollapsed = false) {
  const pageWidth = Math.max(0, width - 300);
  const left = leftCollapsed ? 28 : 230;
  const right = rightCollapsed ? 28 : 280;
  const gap = 12;
  const available = pageWidth - left - right - gap * 2;
  const center = Math.max(0, available);
  return {
    left, center, right, availableHeight: Math.max(0, height - 180),
    overlap: center < 600 || height < 760,
    mapMin: 600,
    pageScroll: false,
  };
}

export const UNKNOWN_VALUE = "— ไม่ทราบ";

export function replayRowFacts(entry: ReplayLibraryEntry) {
  return {
    date: entry.match_date_utc ?? UNKNOWN_VALUE,
    ownHero: entry.own_hero ?? UNKNOWN_VALUE,
    duration: entry.duration_s == null ? UNKNOWN_VALUE : entry.duration_s,
    sizeBytes: entry.bundle_bytes > 0 ? entry.bundle_bytes : null,
  };
}
