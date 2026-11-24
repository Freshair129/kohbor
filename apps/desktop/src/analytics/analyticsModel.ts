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

export function analyticsLayout(width: number, height: number, leftCollapsed = width <= 1500, rightCollapsed = false) {
  const compact = width <= 1500;
  const pageWidth = Math.max(0, width - 72 - 44 - 48);
  const tracks = (collapseLeft: boolean, collapseRight: boolean) => {
    const docs = compact || collapseLeft ? 28 : 273;
    const chat = collapseRight ? 28 : compact ? 289 : 423;
    const centre = Math.max(0, pageWidth - docs - chat - 20);
    const cardWidth = 344 * (compact ? 0.88 : 1);
    const mapWidth = Math.max(0, Math.min(centre - 2 * cardWidth - 34, centre));
    return { docs, centre, chat, rightRail: 44, map: { width: mapWidth, height: mapWidth } };
  };
  const before = tracks(width <= 1500, false);
  const after = tracks(leftCollapsed, rightCollapsed);
  const left = after.docs;
  const center = after.centre;
  const right = after.chat;
  return {
    before, after, left, center, right, rightRail: 44, docs: left, heroCardScale: compact ? 0.88 : 1, availableHeight: Math.max(0, height - 180),
    overlap: center < 600 || height < 760,
    mapMin: 300,
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
