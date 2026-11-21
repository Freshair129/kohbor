import type { EventSoundState } from "./eventMap";

export interface SoundRowPresentation {
  canExpand: boolean;
  showPrimaryPlay: boolean;
  disablePrimaryPlay: boolean;
}

const statesWithoutPrimaryPlay = new Set<EventSoundState>([
  "unmapped",
  "detector_unavailable",
  "not_voiced",
]);

export function soundRowPresentation(state: EventSoundState, variantCount: number): SoundRowPresentation {
  return {
    canExpand: variantCount > 0,
    // A row whose pack has voices keeps its play control even when the detector cannot fire it yet, so the
    // owner can still audition the sound; whether playing is allowed stays with ListenControl.block().
    showPrimaryPlay: variantCount > 0 || !statesWithoutPrimaryPlay.has(state),
    disablePrimaryPlay: false,
  };
}
