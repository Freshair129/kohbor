import type { PlaybackState } from "./playback";
import { overlayChipForDisplay, type ChipView } from "./overlayModel";
import { EventBanner } from "./EventBanner";
import { OverlayChip } from "./OverlayChip";

export function OverlayStage({ playback, chip }: { playback: PlaybackState; chip: ChipView | null }) {
  return (
    <main className="overlay-shell">
      <section className="overlay-stage" aria-label="แบนเนอร์เหตุการณ์">
        <EventBanner playback={playback} />
      </section>
      <div className="overlay-chip-strip">
        <OverlayChip view={overlayChipForDisplay(chip)} />
      </div>
    </main>
  );
}
