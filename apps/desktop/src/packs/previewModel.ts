export interface BrowsePreviewState {
  playbackId: string | null;
  installId: string | null;
  variantId: string | null;
  startedMonotonicMs: number | null;
  receivedAtMs: number | null;
  durationMs: number | null;
  hasVisual: boolean;
  visualUrl: string | null;
  visualFailure: string | null;
}

export type BrowsePreviewEvent =
  | { type: "started"; playbackId: string; installId: string; variantId: string; startedMonotonicMs: number; receivedAtMs: number; durationMs: number; hasVisual: boolean; visualUrl: string | null }
  | { type: "ended"; playbackId: string; reason: string }
  | { type: "visual-failed"; playbackId: string; reason: string }
  | { type: "reset" };

export function createBrowsePreviewState(): BrowsePreviewState {
  return {
    playbackId: null, installId: null, variantId: null, startedMonotonicMs: null,
    receivedAtMs: null, durationMs: null, hasVisual: false, visualUrl: null, visualFailure: null,
  };
}

export function reduceBrowsePreview(state: BrowsePreviewState, event: BrowsePreviewEvent): BrowsePreviewState {
  if (event.type === "reset") return state.playbackId === null ? state : createBrowsePreviewState();
  if (event.type === "started") return {
    playbackId: event.playbackId,
    installId: event.installId,
    variantId: event.variantId,
    startedMonotonicMs: event.startedMonotonicMs,
    receivedAtMs: event.receivedAtMs,
    durationMs: event.durationMs,
    hasVisual: event.hasVisual,
    visualUrl: event.hasVisual ? event.visualUrl : null,
    visualFailure: null,
  };
  if (event.playbackId !== state.playbackId) return state;
  if (event.type === "visual-failed") return { ...state, visualFailure: event.reason, visualUrl: null };
  return createBrowsePreviewState();
}

export function previewElapsedSeconds(state: BrowsePreviewState, nowMs: number): number {
  if (state.receivedAtMs === null || state.durationMs === null) return 0;
  return Math.min(state.durationMs, Math.max(0, nowMs - state.receivedAtMs)) / 1000;
}
