import type { PlaybackState } from "./playback";
import { criticalStripText, overlayChipForDisplay, type ChipView } from "./overlayModel";
import { EventBanner } from "./EventBanner";
import { OverlayChip } from "./OverlayChip";

export function OverlayStage({ playback, chip, visible }: {
  playback: PlaybackState;
  chip: ChipView | null;
  visible: boolean;
}) {
  if (!visible) return null;

  return (
    <main className="overlay-shell">
      <section className="overlay-stage" aria-label="แบนเนอร์เหตุการณ์">
        <EventBanner playback={playback} visible={visible} />
        {playback.criticalStrip ? (
          <div className="critical-strip" data-event-type={playback.criticalStrip.event_type} aria-hidden="true">
            <svg className="critical-strip-glyph" viewBox="0 0 24 24" focusable="false">
              <path d="M12 3 2.5 20h19L12 3Z" />
              <path d="M12 9v5M12 17.2v.3" />
            </svg>
            <span lang="th">{criticalStripText(playback.criticalStrip.event_type)}</span>
          </div>
        ) : null}
      </section>
      <div className="overlay-chip-strip">
        <OverlayChip view={overlayChipForDisplay(chip)} />
      </div>
    </main>
  );
}
