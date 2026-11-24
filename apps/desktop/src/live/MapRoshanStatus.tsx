import type { RoshanObservation } from "./pinnedPanelsModel";
import { deriveRoshanOverlay } from "./pinnedPanelsModel";
import type { LiveLocale } from "./copy";

export function MapRoshanStatus({ observation = null, locale = "th" }: { observation?: RoshanObservation | null; locale?: LiveLocale }) {
  const status = deriveRoshanOverlay(observation);
  const text = status.state === "unknown" && locale === "en" ? "— Unknown" : status.text;
  return <div className="sm-roshan-status" data-state={status.state} data-changed={status.changed} aria-label={`Roshan: ${text}`}>
    <span>Roshan</span><strong>{text}</strong>
  </div>;
}
