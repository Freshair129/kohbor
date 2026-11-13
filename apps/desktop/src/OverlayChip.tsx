import type { ChipView } from "./overlayModel";

export function OverlayChip({ view }: { view: ChipView | null }) {
  if (!view) return null;
  return (
    <span className={`overlay-chip overlay-chip-${view.kind}`} role="status" lang="th">
      {view.text_th}
    </span>
  );
}
