import type { PlaybackState } from "./playback";
import { overlayChipForDisplay, type ChipView } from "./overlayModel";
import { EventBanner } from "./EventBanner";
import { ObjectiveAlert } from "./ObjectiveAlert";
import { OverlayChip } from "./OverlayChip";
import type { Locale } from "./copy";

export function OverlayStage({ playback, chip, visible, objectiveAlertText, objectiveAlertVisible, locale }: {
  playback: PlaybackState;
  chip: ChipView | null;
  visible: boolean;
  objectiveAlertText?: string | null;
  objectiveAlertVisible?: boolean;
  locale?: Locale;
}) {
  if (!visible) return null;

  return (
    <main className="overlay-shell">
      <section className="overlay-stage" aria-label="แบนเนอร์เหตุการณ์">
        <EventBanner playback={playback} visible={visible} />
        <ObjectiveAlert text={objectiveAlertText ?? null} locale={locale ?? "th"} visible={objectiveAlertVisible ?? false} />
      </section>
      <div className="overlay-chip-strip">
        <OverlayChip view={overlayChipForDisplay(chip)} />
      </div>
    </main>
  );
}
