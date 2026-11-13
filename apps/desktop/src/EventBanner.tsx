import type { PlaybackState } from "./playback";
import { toWebviewUrl } from "./mediaUrl";

export function EventBanner({ playback }: { playback: PlaybackState }) {
  const visual = playback.visual;
  if (!visual?.visual_url) return null;
  const platform = typeof navigator !== "undefined" && navigator.userAgent.includes("Windows") ? "windows" : "other";
  const src = toWebviewUrl(visual.visual_url, platform);
  if (!src) return null;

  return (
    <div className="event-overlay-banner" aria-hidden="true">
      <video
        key={visual.playback_id}
        src={src}
        muted
        playsInline
        autoPlay
      />
    </div>
  );
}
