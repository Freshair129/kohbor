export interface BrowsePreviewState {
  playbackId: string | null;
  installId: string | null;
  variantId: string | null;
  soundKey: string | null;
  playing: boolean;
  durationMs: number | null;
  hasVisual: boolean;
  visualUrl: string | null;
  visualFailure: string | null;
}

export type BrowsePreviewEvent =
  | { type: "started"; playbackId: string; installId: string; variantId: string; soundKey: string; durationMs: number; hasVisual: boolean; visualUrl: string | null }
  | { type: "ended"; playbackId: string; reason: string }
  | { type: "visual-failed"; playbackId: string; reason: string }
  | { type: "reset" };

export function createBrowsePreviewState(): BrowsePreviewState {
  return {
    playbackId: null, installId: null, variantId: null, soundKey: null, playing: false,
    durationMs: null, hasVisual: false, visualUrl: null, visualFailure: null,
  };
}

export function reduceBrowsePreview(state: BrowsePreviewState, event: BrowsePreviewEvent): BrowsePreviewState {
  if (event.type === "reset") return state.playbackId === null ? state : createBrowsePreviewState();
  if (event.type === "started") return {
    playbackId: event.playbackId,
    installId: event.installId,
    variantId: event.variantId,
    soundKey: event.soundKey,
    playing: true,
    durationMs: event.durationMs,
    hasVisual: event.hasVisual,
    visualUrl: event.hasVisual ? event.visualUrl : null,
    visualFailure: event.hasVisual && !event.visualUrl ? "VIDEO_UNAVAILABLE" : null,
  };
  if (event.playbackId !== state.playbackId) return state;
  if (event.type === "visual-failed") return { ...state, visualFailure: event.reason, visualUrl: null };
  if (!state.playing) return state;
  return { ...state, playing: false };
}

/** Prepare a muted first frame before starting the native audition clock. */
export function prepareBrowseVideo(video: HTMLVideoElement, url: string): Promise<boolean> {
  video.pause();
  return new Promise(resolve => {
    const finish = (ready: boolean) => {
      clearTimeout(timer);
      video.removeEventListener("canplay", readyToPlay);
      video.removeEventListener("error", failed);
      resolve(ready);
    };
    const readyToPlay = () => finish(true);
    const failed = () => finish(false);
    const timer = setTimeout(failed, 2000);
    video.addEventListener("canplay", readyToPlay);
    video.addEventListener("error", failed);
    video.src = url;
    video.load();
  });
}

export async function playBrowseVideo(video: Pick<HTMLVideoElement, "currentTime" | "play">): Promise<void> {
  if (video.currentTime !== 0) video.currentTime = 0;
  return video.play();
}

/** Seek to the last displayable frame on completion. A cut holds the current frame. */
export function holdBrowseVideo(video: Pick<HTMLVideoElement, "currentTime" | "duration" | "pause">, finished: boolean) {
  video.pause();
  if (finished && Number.isFinite(video.duration) && video.duration > 0) {
    video.currentTime = Math.max(0, video.duration - 0.001);
  }
}
