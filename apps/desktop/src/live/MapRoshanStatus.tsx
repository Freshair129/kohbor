import type { RoshanObservation } from "./pinnedPanelsModel";
import { deriveRoshanOverlay } from "./pinnedPanelsModel";
import type { LiveLocale } from "./copy";

export function MapRoshanStatus({ observation = null, locale = "th" }: { observation?: RoshanObservation | null; locale?: LiveLocale }) {
  // No Roshan data source exists yet (observation is always null), so a permanent "unknown" chip only adds noise.
  if (observation === null) return null;
  const status = deriveRoshanOverlay(observation);
  const text = status.state === "unknown" && locale === "en" ? "— Unknown" : status.text;
  return <div className="sm-roshan-status" data-state={status.state} data-changed={status.changed} aria-label={`Roshan: ${text}`}>
    <span>Roshan</span><strong>{text}</strong>
  </div>;
}
