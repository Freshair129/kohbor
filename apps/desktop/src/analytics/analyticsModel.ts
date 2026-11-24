import type { ReplayLibraryEntry } from "../replay/replayLibraryClient";
import type { MatchBundle } from "../replay/matchBundle";

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
  const tracks = (viewportWidth: number, railWidth: number, collapseLeft: boolean, collapseRight: boolean) => {
    const compact = width <= 1500;
    const specs = compact
      ? { left: [190, 0.72], center: [600, 1.85], right: [240, 0.85] }
      : { left: [210, 0.78], center: [600, 1.85], right: [260, 0.9] };
    const afterSpecs = compact
      ? { left: [190, 0.72], center: [600, 1.85], right: [144, 0.26] }
      : { left: [210, 0.78], center: [600, 1.85], right: [156, 0.38] };
    const selectedSpecs = railWidth === 44 ? afterSpecs : specs;
    const pageWidth = Math.max(0, viewportWidth - 72 - railWidth - 48);
    const gap = 10;
    const leftMin = collapseLeft ? 28 : selectedSpecs.left[0];
    const rightMin = collapseRight ? 28 : selectedSpecs.right[0];
    const centerMin = collapseLeft && collapseRight ? 0 : selectedSpecs.center[0];
    const weights = [collapseLeft ? 0 : selectedSpecs.left[1], collapseLeft && collapseRight ? 1 : selectedSpecs.center[1], collapseRight ? 0 : selectedSpecs.right[1]];
    const mins = [leftMin, centerMin, rightMin];
    const free = Math.max(0, pageWidth - gap * 2 - mins.reduce((sum, value) => sum + value, 0));
    const weightTotal = weights.reduce((sum, value) => sum + value, 0) || 1;
    const columns = mins.map((value, index) => value + free * weights[index]! / weightTotal);
    const center = collapseLeft && collapseRight ? Math.max(0, pageWidth - 56 - gap * 2) : columns[1]!;
    const mapWidth = Math.max(0, 300 + Math.max(0, center - 18 - 16 - 580) * 1.35 / 3.35 - 20);
    return {
      docs: columns[0]!, centre: center, chat: columns[2]!, rightRail: railWidth,
      map: { width: mapWidth, height: mapWidth },
    };
  };
  const before = tracks(width, 72, false, false);
  const after = tracks(width, 44, leftCollapsed, rightCollapsed);
  const left = after.docs;
  const center = after.centre;
  const right = after.chat;
  return {
    before, after, left, center, right, availableHeight: Math.max(0, height - 180),
    overlap: center < 600 || height < 760,
    mapMin: 600,
    pageScroll: center < 600 || height < 760,
  };
}

export const UNKNOWN_VALUE = "— ไม่ทราบ";
export const ANALYTICS_PLAYBACK_SPEEDS = [1, 5, 10, 30] as const;

export function formatAnalyticsTime(seconds: number | null): string {
  if (seconds === null || !Number.isFinite(seconds)) return UNKNOWN_VALUE;
  const rounded = Math.round(seconds);
  const absolute = Math.abs(rounded);
  return `${rounded < 0 ? "−" : ""}${String(Math.floor(absolute / 60)).padStart(2, "0")}:${String(absolute % 60).padStart(2, "0")}`;
}

export function analyticsPlaybackKeyAction(key: string, targetType?: string | null) {
  if (["input", "select", "textarea", "button"].includes((targetType ?? "").toLowerCase()) && targetType !== "range") return null;
  if (key === " " || key === "Space" || key === "Spacebar") return { type: "toggle" as const };
  if (key === "ArrowLeft") return { type: "seek" as const, deltaSeconds: -5 };
  if (key === "ArrowRight") return { type: "seek" as const, deltaSeconds: 5 };
  return null;
}

export function analyticsDeathTeam(death: { hero?: unknown }, heroes: readonly { id?: unknown; team?: unknown }[]): 2 | 3 | null {
  if (typeof death.hero !== "string") return null;
  const team = heroes.find(hero => hero.id === death.hero)?.team;
  return team === 2 || team === 3 ? team : null;
}

export type AnalyticsReturnContext = { matchId: string; timeS: number; documentId: string };
export type AnalyticsReplayDetailsRoute = {
  matchId: string;
  bundle: MatchBundle;
  initialTime: number;
  returnTo: AnalyticsReturnContext;
};

export function enterAnalyticsReplayDetails(bundle: MatchBundle, context: AnalyticsReturnContext): AnalyticsReplayDetailsRoute {
  return { matchId: context.matchId, bundle, initialTime: context.timeS, returnTo: { ...context } };
}

export function returnToAnalytics(route: AnalyticsReplayDetailsRoute): AnalyticsReturnContext {
  return { ...route.returnTo };
}

export function replayRowFacts(entry: ReplayLibraryEntry) {
  return {
    date: entry.match_date_utc ?? UNKNOWN_VALUE,
    ownHero: entry.own_hero ?? UNKNOWN_VALUE,
    duration: entry.duration_s == null ? UNKNOWN_VALUE : entry.duration_s,
    sizeBytes: entry.bundle_bytes > 0 ? entry.bundle_bytes : null,
  };
}
