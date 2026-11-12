import type { PlaybackState } from "./playback";

export function EventBanner({ playback }: { playback: PlaybackState }) {
  const visual = playback.visual;
  if (!visual?.visual_url) return null;

  return (
    <div className="event-overlay-banner" aria-hidden="true">
      <video
        key={visual.playback_id}
        src={visual.visual_url}
        muted
        playsInline
        autoPlay
      />
    </div>
  );
}
