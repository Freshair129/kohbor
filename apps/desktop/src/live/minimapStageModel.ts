// @ts-ignore Explicit extensions support Node's TypeScript loader.
import type { MinimapState, SteamStatus } from "../steam/model.ts";
export type { MinimapState };

/** Keep capture cases aligned with steam.rs::minimap_placeholder. Process/draft branches
 * belong to the host; without steam_status, idle alone cannot prove Dota is not running. */
export function deriveMinimapState(steam: SteamStatus | null, capture: { phase: string; phase_reason: string | null } | null, locallyBlocked = false): MinimapState {
  // Local pause/invalidation must keep their existing reason and retry controls visible.
  if (locallyBlocked) return "unsupported";
  if (steam) return steam.minimap_state;
  if (capture?.phase === "live") return "live";
  if (capture?.phase === "stale" && capture.phase_reason === "occluded") return "occluded";
  if (capture?.phase === "idle") return "loading";
  if (capture?.phase === "calibrating") return "first_frame";
  return "unsupported";
}
export interface RevealState {
  source: MinimapState;
  view: MinimapState;
  frameReady: boolean;
  reducedMotion: boolean;
  revealId: number;
  pendingReveal: boolean;
}
type RevealEvent = { type: "sync"; source: MinimapState; hasFrame: boolean; reducedMotion: boolean } | { type: "finish"; revealId: number };
export const createRevealState = (): RevealState => ({ source: "loading", view: "loading", frameReady: false, reducedMotion: false, revealId: 0, pendingReveal: false });
export const revealDuration = (state: RevealState): number => state.reducedMotion ? 160 : 760;

/** hasFrame means useMinimap has painted an admitted frame, never just received bytes. */
export function reduceReveal(state: RevealState, event: RevealEvent): RevealState {
  if (event.type === "finish") return state.view === "first_frame" && state.frameReady && event.revealId === state.revealId
    ? { ...state, view: "live", pendingReveal: false } : state;
  const next = { ...state, source: event.source, frameReady: event.hasFrame, reducedMotion: event.reducedMotion };
  if (event.source !== "first_frame" && event.source !== "live") return { ...next, view: event.source, pendingReveal: false };
  const pending = state.pendingReveal || event.source === "first_frame";
  if (!event.hasFrame) return { ...next, view: "loading", pendingReveal: pending };
  if (state.view === "first_frame") return { ...next, pendingReveal: false };
  if (event.source === "first_frame" && state.source === "first_frame" && state.view === "live") return { ...next, pendingReveal: false };
  if (pending) return { ...next, view: "first_frame", revealId: state.revealId + 1, pendingReveal: false };
  return { ...next, view: "live", pendingReveal: false };
}
export function minimapPlaceholderText(state: MinimapState, locale: "th" | "en", waitingReason: string | null = null): string {
  if (state === "loading" && waitingReason) return waitingReason;
  const messages = locale === "th" ? {
    not_running: "Dota 2 ปิดอยู่ — กดเริ่ม", launching: "กำลังเปิด Dota 2…", hero_selection: "กำลังเลือกฮีโร่",
    loading: "กำลังโหลดเกม", first_frame: "กำลังเปิดภาพแผนที่", live: "กำลังรับภาพ", occluded: "รอเฟรมแผนที่", unsupported: "ยังแสดงภาพไม่ได้",
  } : {
    not_running: "Dota 2 is closed — press Start", launching: "Launching Dota 2…", hero_selection: "Choosing a hero",
    loading: "Loading the game", first_frame: "Opening minimap", live: "Receiving minimap", occluded: "Waiting for minimap frame", unsupported: "Minimap unavailable",
  };
  return messages[state];
}
