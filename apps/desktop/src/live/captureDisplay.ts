// @ts-ignore Explicit extensions support Node's TypeScript loader.
import type { CaptureStatus } from "./types.ts";
// @ts-ignore Explicit extensions support Node's TypeScript loader.
import type { PreviewMetrics } from "./preview.ts";

type DisplayMetrics = Omit<PreviewMetrics, "received_total" | "painted_total"> & {
  received_total: number | null;
  painted_total: number | null;
};

/** Idle has no current capture measurements; never present retained counters as active. */
export function deriveCaptureDisplay(capture: CaptureStatus | null, metrics: PreviewMetrics): {
  capture: CaptureStatus | null; metrics: DisplayMetrics;
} {
  const previewActive = capture !== null && (capture.phase === "live" || capture.phase === "stale") && capture.paused !== true;
  const displayedMetrics: DisplayMetrics = previewActive ? metrics : {
    received_fps: null, painted_fps: null, dropped_count: null, parse_paint_ms_mean: null,
    received_total: null, painted_total: null,
  };
  if (capture?.phase !== "idle") return { capture, metrics: displayedMetrics };
  return {
    capture: { ...capture, fps_measured: null, grab_fps: null, grab_timeouts_per_s: null, sent_total: null, grab_total: null,
      frame_age_ms: null, grab_ms_mean: null, analysis_ms_mean: null },
    metrics: displayedMetrics,
  };
}
