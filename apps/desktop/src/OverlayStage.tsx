import type { PlaybackState } from "./playback";
import type { ChipView } from "./overlayModel";
import { EventBanner } from "./EventBanner";
import { OverlayChip } from "./OverlayChip";
import { RiskStrip } from "./live";

export function OverlayStage({ playback, chip }: { playback: PlaybackState; chip: ChipView | null }) {
  return (
    <main className="overlay-shell">
      <section className="overlay-stage" aria-label="แบนเนอร์เหตุการณ์">
        <EventBanner playback={playback} />
        <div id="overlay-risk-strip" className="overlay-risk-strip">
          <RiskStrip />
        </div>
      </section>
      <div className="overlay-chip-strip">
        <OverlayChip view={chip} />
      </div>
    </main>
  );
}
