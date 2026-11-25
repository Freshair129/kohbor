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

export type AnalyticsLayoutMode = "docked" | "drawer" | "enlarge-window";
const DOCKED_CENTER_MIN = 1004;
const DRAWER_CENTER_MIN = 938;
const DRAWER_CHAT_SWITCH = 1501;
const LAYOUT_GAP = 20;

/** Choose columns from the measured Analytics page size, never the outer viewport. */
export function analyticsLayout(
  pageWidth: number,
  pageHeight: number,
  leftCollapsed = pageWidth < 273 + DOCKED_CENTER_MIN + 423 + LAYOUT_GAP,
  rightCollapsed = false,
  readingMode = false,
) {
  const width = Number.isFinite(pageWidth) ? Math.max(0, pageWidth) : 0;
  const height = Number.isFinite(pageHeight) ? Math.max(0, pageHeight) : 0;
  const dockedDocs = readingMode ? 420 : 273;
  const dockedMin = dockedDocs + DOCKED_CENTER_MIN + 423 + LAYOUT_GAP;
  const drawerChat = width >= DRAWER_CHAT_SWITCH ? 423 : 289;
  const mode: AnalyticsLayoutMode = height < 760
    ? "enlarge-window"
    : width >= dockedMin
      ? "docked"
      : width >= 28 + DRAWER_CENTER_MIN + drawerChat + LAYOUT_GAP
        ? "drawer"
        : "enlarge-window";
  const heroCardScale = mode === "docked" ? 1 : 0.88;
  const docs = mode === "docked" ? dockedDocs : 28;
  const chat = rightCollapsed ? 28 : mode === "docked" ? 423 : mode === "drawer" ? drawerChat : 289;
  const centre = Math.max(0, width - (leftCollapsed ? 28 : docs) - chat - LAYOUT_GAP);
  const cardWidth = 344 * heroCardScale;
  const mapWidth = Math.max(0, Math.min(centre - 2 * cardWidth - 34, centre));
  const track = { docs: leftCollapsed ? 28 : docs, centre, chat, rightRail: 44, map: { width: mapWidth, height: mapWidth } };
  const expandedTrack = { ...track, docs: leftCollapsed ? 28 : docs, chat: rightCollapsed ? 28 : chat };
  const compactTrack = { ...track, docs: 28, chat: mode === "docked" ? 423 : mode === "drawer" ? drawerChat : 289 };
  return {
    mode,
    before: compactTrack,
    after: expandedTrack,
    left: expandedTrack.docs,
    center: centre,
    right: chat,
    rightRail: 44,
    docs: expandedTrack.docs,
    drawerWidth: Math.min(readingMode ? 640 : 420, Math.max(0, width - chat - 38)),
    minCenter: mode === "docked" ? DOCKED_CENTER_MIN : DRAWER_CENTER_MIN,
    heroCardScale,
    availableHeight: Math.max(0, height - 180),
    overlap: false,
    mapMin: 300,
    pageScroll: mode === "enlarge-window",
  };
}

export const ANALYTICS_TABS = [
  ["summary", "สรุป"],
  ["compare", "เทียบ"],
  ["ai", "AI"],
  ["notes", "โน้ต"],
] as const;

export function analyticsConsentRePrompt(
  record: { text_version?: number } | null | undefined,
  currentVersion: number,
  alreadyNotified = false,
) {
  const stale = typeof record?.text_version === "number" && record.text_version < currentVersion;
  const noticeKey = stale ? `${record!.text_version}:${currentVersion}` : null;
  const show = stale && !alreadyNotified;
  return { showBanner: show, openDialog: show, noticeKey };
}

export function analyticsReplaySelection(entry: { match_id: string }) {
  return { selectedId: entry.match_id, parseOnSelect: false as const };
}

/** A library refresh should reopen a bundle only after this selected identity changes. */
export function analyticsBundleSelectionKey(entry: Pick<ReplayLibraryEntry,
  "match_id" | "status" | "dem_sha256" | "engine_sha256" | "bundle_schema"> & { bundle_hash?: string | null } | null | undefined): string | null {
  if (!entry) return null;
  return JSON.stringify([entry.match_id, entry.status.state, entry.bundle_hash ?? null, entry.dem_sha256, entry.engine_sha256, entry.bundle_schema ?? null]);
}

export function analyticsReplayAction(status: { state: string }) {
  if (status.state === "available") return "parse" as const;
  if (status.state === "failed") return "retry" as const;
  if (status.state === "ready") return "reparse" as const;
  return null;
}

export function analyticsAiVisibility(developerMode: boolean, showUntested: boolean) {
  return {
    showUntestedControl: developerMode,
    showUntested: developerMode && showUntested,
    chatCollapsed: !developerMode,
    showUnavailableNotice: !developerMode,
  };
}

export type AnalyticsDeleteState = "closed" | "confirming" | "deleting";
export type AnalyticsDeleteEvent = "request" | "cancel" | "confirm" | "complete";

export function analyticsDeleteTransition(
  state: AnalyticsDeleteState,
  event: AnalyticsDeleteEvent,
  matchId: string | null,
): { state: AnalyticsDeleteState; command: { name: string; args: { matchId: string } } | null } {
  if (event === "request" && state === "closed") return { state: "confirming", command: null };
  if (event === "cancel" && state === "confirming") return { state: "closed", command: null };
  if (event === "confirm" && state === "confirming" && matchId) {
    return { state: "deleting", command: { name: "analytics_match_data_delete", args: { matchId } } };
  }
  if (event === "complete" && state === "deleting") return { state: "closed", command: null };
  return { state, command: null };
}

export const ANALYTICS_ORPHANS_COMMAND = "analytics_orphans_list";

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

export type AnalyticsChatTurn = { role: "user" | "assistant"; content: string };

/** Resolve a safe #doc target to an in-panel destination. */
export function analyticsDocumentDestination(id: string, noteIds: readonly string[]): "report" | "note" | null {
  if (id === "ai-report") return "report";
  return noteIds.includes(id) ? "note" : null;
}

/** Build a bounded recap from visible turns without making a model request. */
export function buildChatRecap(turns: readonly AnalyticsChatTurn[], maxCharacters = 12_000): string {
  const limit = Math.max(160, Math.min(64 * 1024, Math.floor(maxCharacters)));
  const header = "# Chat recap\n\nCreated locally without an AI request.\n\n";
  const rows = turns.slice(-24).map(turn => {
    const role = turn.role === "user" ? "User" : "Assistant";
    const content = turn.content.replace(/\s+/g, " ").trim();
    return content ? `**${role}:** ${content}` : "";
  }).filter(Boolean);
  let recap = header;
  for (const row of rows) {
    const next = `${recap}${recap.endsWith("\n\n") ? "" : "\n\n"}${row}`;
    if (next.length > limit) {
      const available = Math.max(0, limit - recap.length - 16);
      if (available > 0) recap += `${recap.endsWith("\n\n") ? "" : "\n\n"}${row.slice(0, available)}… [truncated]`;
      break;
    }
    recap = next;
  }
  return recap.slice(0, limit);
}

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
