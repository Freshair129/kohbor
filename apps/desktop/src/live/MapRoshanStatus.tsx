import { formatRuneCountdown, type RoshanPit, type RoshanPitStatus } from "./objectiveCountdownModel";
import type { RoshanObservation } from "./pinnedPanelsModel";
import { deriveRoshanOverlay } from "./pinnedPanelsModel";
import { liveCopy, type LiveLocale } from "./copy";

type RoshanSpot = { x: number; y: number; worldX: number; worldY: number };

export function MapRoshanStatus({
  status = null,
  spots,
  comparisonPatch = null,
  observation = null,
  locale = "th",
}: {
  status?: RoshanPitStatus | null;
  spots?: Readonly<Record<RoshanPit, RoshanSpot>>;
  comparisonPatch?: string | null;
  observation?: RoshanObservation | null;
  locale?: LiveLocale;
}) {
  if (status && status.state !== "unknown" && spots) {
    const copy = liveCopy(locale).pinnedPanels;
    const spot = spots[status.pit];
    const patchDiffers = comparisonPatch !== null && comparisonPatch !== status.patchVersion;
    const timer = status.state === "moving"
      ? String(Math.ceil(status.secondsRemaining)).padStart(2, "0")
      : formatRuneCountdown(status.secondsUntilSwitch);
    const phaseText = status.state === "moving"
      ? copy.roshanMoving(Math.ceil(status.secondsRemaining))
      : copy.roshanSwitchIn(timer ?? "—");
    const pitText = copy.roshanPit(status.pit.toUpperCase());
    const source = copy.roshanMeasured(status.patchVersion, status.sampleCount);
    const mismatch = patchDiffers && comparisonPatch
      ? copy.roshanPatchMismatch(comparisonPatch, status.patchVersion) : "";
    const tooltip = [pitText, phaseText, source, mismatch].filter(Boolean).join(" · ");
    return (
      <div className="sm-roshan-marker" style={{ left: `${spot.x * 100}%`, top: `${spot.y * 100}%` }}
        data-pit={status.pit} data-state={status.state} data-position-patch-differs={patchDiffers || undefined}
        aria-label={tooltip} title={tooltip}>
        <span className="sm-roshan-icon" aria-hidden="true">
          <svg viewBox="0 0 24 24" focusable="false">
            <path d="M4.1 18.8 2.8 9.7l4.7 3.1L8.1 5l3.9 3.1L16 5l.6 7.8 4.7-3.1-1.3 9.1c-3.8 2.2-8.1 2.4-11.9 0Z" />
            <circle cx="9" cy="15" r="1" /><circle cx="15" cy="15" r="1" />
          </svg>
        </span>
        {timer !== null && <span className="sm-roshan-countdown" aria-hidden="true">{timer}</span>}
      </div>
    );
  }

  // Preserve the older explicit observation view for callers outside the pinned map layer.
  if (!observation) return null;
  const statusView = deriveRoshanOverlay(observation);
  const text = statusView.state === "unknown" && locale === "en" ? "— Unknown" : statusView.text;
  return <div className="sm-roshan-status" data-state={statusView.state} data-changed={statusView.changed} aria-label={`Roshan: ${text}`}>
    <span>Roshan</span><strong>{text}</strong>
  </div>;
}
