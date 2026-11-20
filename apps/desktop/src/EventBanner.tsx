import { useCallback, useEffect, useRef } from "react";
import type { PlaybackState } from "./playback";
import { toWebviewUrl } from "./mediaUrl";
import { emitOverlayVideoDiagnostic } from "./runtime";

export function EventBanner({ playback }: { playback: PlaybackState }) {
  const reportedPlayback = useRef<string | null>(null);
  const visual = playback.visual;
  const playbackId = visual?.playback_id ?? null;
  const eventType = visual?.event_type ?? null;
  const platform = typeof navigator !== "undefined" && navigator.userAgent.includes("Windows") ? "windows" : "other";
  const src = visual?.visual_url ? toWebviewUrl(visual.visual_url, platform) : null;

  const report = useCallback((
    outcome: "started" | "failed",
    errorCode: number | null,
    reasonCode: "NONE" | "VIDEO_ERROR" | "VIDEO_URL_UNAVAILABLE" | "VIDEO_START_TIMEOUT",
  ) => {
    if (!playbackId || reportedPlayback.current === playbackId) return;
    reportedPlayback.current = playbackId;
    void emitOverlayVideoDiagnostic({
      playback_id: playbackId,
      event_type: eventType,
      outcome,
      error_code: errorCode,
      reason_code: reasonCode,
    });
  }, [eventType, playbackId]);

  useEffect(() => {
    if (!playbackId || reportedPlayback.current === playbackId) return;
    if (!src) {
      report("failed", null, "VIDEO_URL_UNAVAILABLE");
      return;
    }
    // Keep this bounded diagnostic alive through a short clip's unmount so a
    // silent autoplay stall still reaches the host. Duplicate StrictMode timers
    // are harmless because the playback ID is emitted once.
    window.setTimeout(() => report("failed", null, "VIDEO_START_TIMEOUT"), 5_000);
  }, [playbackId, report, src]);

  if (!visual?.visual_url || !src) return null;

  return (
    <div className="event-overlay-banner" aria-hidden="true">
      <video
        key={visual.playback_id}
        src={src}
        muted
        playsInline
        autoPlay
        onPlaying={() => report("started", null, "NONE")}
        onError={(event) => report("failed", event.currentTarget.error?.code ?? null, "VIDEO_ERROR")}
      />
    </div>
  );
}
