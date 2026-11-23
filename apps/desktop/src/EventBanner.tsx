import { useCallback, useEffect, useRef } from "react";
import type { PlaybackState } from "./playback";
import { toWebviewUrl } from "./mediaUrl";
import { emitOverlayVideoDiagnostic } from "./runtime";
import { createPlaybackReporter, type VideoReason } from "./playbackReporter";

export function EventBanner({ playback, visible = true }: { playback: PlaybackState; visible?: boolean }) {
  const eventTypes = useRef(new Map<string, string | null>());
  const reporter = useRef(createPlaybackReporter((id, outcome, errorCode, reasonCode) => {
    void emitOverlayVideoDiagnostic({
      playback_id: id,
      event_type: eventTypes.current.get(id) ?? null,
      outcome,
      error_code: errorCode,
      reason_code: reasonCode,
    });
    eventTypes.current.delete(id);
  }));
  const visual = playback.visual;
  const playbackId = visual?.playback_id ?? null;
  const eventType = visual?.event_type ?? null;
  const platform = typeof navigator !== "undefined" && navigator.userAgent.includes("Windows") ? "windows" : "other";
  const src = visual?.visual_url ? toWebviewUrl(visual.visual_url, platform) : null;

  const report = useCallback((id: string | null, outcome: "started" | "failed", errorCode: number | null, reasonCode: VideoReason) => {
    reporter.current(id, outcome, errorCode, reasonCode);
  }, []);

  useEffect(() => {
    if (!playbackId) return;
    eventTypes.current.set(playbackId, eventType);
    if (!src) {
      report(playbackId, "failed", null, "VIDEO_URL_UNAVAILABLE");
      return;
    }
    // The timer is bound to THIS playback ID and outlives a short clip's unmount, so a silent autoplay stall still
    // reaches the host; once this playback reported started or failed, the timer does nothing.
    const id = playbackId;
    window.setTimeout(() => report(id, "failed", null, "VIDEO_START_TIMEOUT"), 5_000);
  }, [eventType, playbackId, report, src]);

  if (!visible || !visual?.visual_url || !src) return null;

  const classes = [
    "event-overlay-banner",
    playback.visualYielding ? "is-yielding" : "",
    playback.visualFadeOutAtMs !== null ? "is-fading-out" : "",
  ].filter(Boolean).join(" ");

  return (
    <div className={classes} aria-hidden="true">
      <video
        key={visual.playback_id}
        src={src}
        muted
        playsInline
        autoPlay
        onPlaying={() => report(visual.playback_id, "started", null, "NONE")}
        onError={(event) => report(visual.playback_id, "failed", event.currentTarget.error?.code ?? null, "VIDEO_ERROR")}
      />
    </div>
  );
}
