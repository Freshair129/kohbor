import { useEffect, useState } from "react";
import { countdownLabels, objectiveShortName, spotLabels, type CountdownObjective, type ObjectiveSchedule } from "./objectiveCountdownModel";

/** Original markers (no game art): a ring with a short letter per rune type. */
const MARK: Record<CountdownObjective, string> = { power_rune: "P", water_rune: "W", bounty_rune: "B", wisdom_rune: "X" };

/**
 * Re-renders once a second so a stale schedule disappears even when no new snapshot arrives. The time used is read at
 * render, so a schedule that arrived after the last tick is never treated as coming from the future.
 */
function useNow(active: boolean): number {
  const [, setTick] = useState(0);
  useEffect(() => {
    if (!active) return;
    const timer = window.setInterval(() => setTick(tick => tick + 1), 1_000);
    return () => window.clearInterval(timer);
  }, [active]);
  return Date.now();
}

export function MapCountdowns({ schedule, receivedAtMs, locale }: {
  schedule: ObjectiveSchedule | null; receivedAtMs: number | null; locale: "th" | "en";
}) {
  const now = useNow(schedule !== null);
  const spots = spotLabels(countdownLabels(schedule, receivedAtMs, now));
  if (spots.length === 0) return null;
  return (
    <ul className="sm-countdowns" aria-label={locale === "th" ? "นับถอยหลังรูน" : "Rune countdowns"}>
      {spots.map(spot => (
        <li key={spot.key} className="sm-countdown" data-objective={spot.label.objective} data-mode={spot.label.mode}
          style={{ left: `${spot.x * 100}%`, top: `${spot.y * 100}%` }}
          aria-label={`${objectiveShortName(spot.label.objective, locale)} ${spot.label.text}`}>
          <span className="sm-countdown-mark" aria-hidden="true">{MARK[spot.label.objective]}</span>
          <span className="sm-countdown-text mono">{spot.label.text}</span>
        </li>
      ))}
    </ul>
  );
}
