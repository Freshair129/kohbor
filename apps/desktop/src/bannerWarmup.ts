/**
 * One-shot warm-up of the overlay's banner video path.
 *
 * In game the first banner after each Kohbor start dropped 7-11 frames with 287-300 ms stalls while
 * later banners played clean (native measurement 2026-11-28). Playing one tiny, original, transparent
 * VP9-with-alpha clip once, invisibly, before any banner moves that cold cost off the first banner
 * (first banners then dropped 0/96 and 0/106). The clip is the bundled `assets/overlay/banner-warmup.webm`;
 * see `tools/banner-warmup/` for how it was generated.
 *
 * The element is 1x1 CSS px at opacity 0.05: a fully transparent layer is culled by the compositor and
 * warms nothing, so the layer must stay drawable. The clip itself is near-transparent (max alpha 23/255).
 */

/** Inline style for the warm-up element; kept as one string so tests can pin it. */
export const WARMUP_STYLE =
  "position:fixed;left:0;top:0;width:1px;height:1px;opacity:0.05;pointer-events:none;z-index:0";

/** Upper bound for one warm-up; the bundled clip lasts 2 s. */
export const WARMUP_MAX_MS = 4_000;

interface WarmupVideo {
  muted: boolean;
  playsInline: boolean;
  preload: string;
  src: string;
  style: { cssText: string };
  setAttribute(name: string, value: string): void;
  removeAttribute(name: string): void;
  addEventListener(type: "ended" | "error", listener: () => void, options?: { once?: boolean }): void;
  play(): Promise<void> | undefined;
  pause(): void;
  load(): void;
  remove(): void;
}

export interface WarmupDeps {
  createVideo(): WarmupVideo;
  append(video: WarmupVideo): void;
  setTimer(callback: () => void, ms: number): unknown;
  clearTimer(handle: unknown): void;
}

function browserDeps(): WarmupDeps | null {
  if (typeof document === "undefined" || !document.body) return null;
  return {
    createVideo: () => document.createElement("video") as unknown as WarmupVideo,
    append: (video) => document.body.appendChild(video as unknown as Node),
    setTimer: (callback, ms) => window.setTimeout(callback, ms),
    clearTimer: (handle) => window.clearTimeout(handle as number),
  };
}

/**
 * Play `src` once, invisibly, then remove the element. Returns a stop function that is safe to call
 * any number of times (also after the warm-up finished on its own).
 */
export function startBannerWarmup(src: string, deps: WarmupDeps | null = browserDeps()): () => void {
  if (!deps) return () => {};
  const video = deps.createVideo();
  let done = false;
  let timer: unknown = null;

  const stop = () => {
    if (done) return;
    done = true;
    if (timer !== null) deps.clearTimer(timer);
    try {
      video.pause();
      video.removeAttribute("src");
      video.load();
    } finally {
      video.remove();
    }
  };

  video.muted = true;
  video.playsInline = true;
  video.preload = "auto";
  video.setAttribute("aria-hidden", "true");
  video.setAttribute("data-banner-warmup", "");
  video.style.cssText = WARMUP_STYLE;
  video.addEventListener("ended", stop, { once: true });
  video.addEventListener("error", stop, { once: true });
  video.src = src;
  deps.append(video);
  timer = deps.setTimer(stop, WARMUP_MAX_MS);
  const playing = video.play();
  if (playing) playing.catch(stop);
  return stop;
}
