import { useEffect, useState } from "react";
import { countdownLabels, objectiveShortName, RUNE_SPOT_PROVENANCE, spotLabels, type CountdownObjective, type ObjectiveSchedule } from "./objectiveCountdownModel";

/** Time to spawn as m:ss ("0:09"); the model keeps whole seconds. */
function clock(seconds: number): string {
  const s = Math.max(0, Math.round(seconds));
  return `${Math.floor(s / 60)}:${String(s % 60).padStart(2, "0")}`;
}

/** Owner feedback 2026-11-25: "B / P / กติกา 7.42" was unreadable. Each entry now names the rune in words with a coloured
 * dot (original marker, no game art) and a m:ss time; the patch the timing rule comes from moves to the tooltip. */

function markerPlacement(x: number, y: number): "upper-left" | "upper-right" | "lower-left" | "lower-right" {
  if (y < 0.35 && x < 0.5) return "upper-left";
  if (y > 0.65 && x > 0.5) return "lower-right";
  return x < 0.5 ? "upper-right" : "lower-left";
}

/**
 * Re-renders once a second so a stale schedule disappears even when no new snapshot arrives. The time used is read at
 * render, so a schedule that arrived after the last tick is never treated as coming from the future.
 */
function useNow(active: boolean): number {
  const [, setTick] = useState(0);
  const [visible, setVisible] = useState(() => typeof document === "undefined" || document.visibilityState === "visible");
  useEffect(() => {
    const onVisibility = () => setVisible(document.visibilityState === "visible");
    document.addEventListener("visibilitychange", onVisibility);
    return () => document.removeEventListener("visibilitychange", onVisibility);
  }, []);
  useEffect(() => {
    if (!active || !visible) return;
    const timer = window.setInterval(() => setTick(tick => tick + 1), 1_000);
    return () => window.clearInterval(timer);
  }, [active, visible]);
  return Date.now();
}

export function MapCountdowns({ schedule, receivedAtMs, locale, gameClockSeconds = null }: {
  schedule: ObjectiveSchedule | null; receivedAtMs: number | null; locale: "th" | "en"; gameClockSeconds?: number | null;
}) {
  const now = useNow(schedule !== null);
  const clockSchedule = schedule && Number.isSafeInteger(gameClockSeconds) && gameClockSeconds !== null
    ? { ...schedule, observed_clock_seconds: gameClockSeconds } : schedule;
  const spots = spotLabels(countdownLabels(clockSchedule, receivedAtMs, now));
  if (spots.length === 0) return null;
  return (
    <ul className="sm-countdowns" aria-label={locale === "th" ? "นับถอยหลังรูน" : "Rune countdowns"}>
      {spots.map(spot => (
        <li key={spot.key} className="sm-countdown-spot" style={{ left: `${spot.x * 100}%`, top: `${spot.y * 100}%` }}
          aria-label={spot.labels.map(label => `${objectiveShortName(label.objective, locale)} ${label.mode === "ready" ? label.text : clock(label.remaining)}`).join(", ")}
          title={locale === "th"
            ? `เวลาเกิดตามกติกาแพตช์ ${spot.labels.find(label => label.patch_version)?.patch_version ?? "—"} · ตำแหน่งจากรีเพลย์แพตช์ ${RUNE_SPOT_PROVENANCE.patch}`
            : `Spawn timing from patch ${spot.labels.find(label => label.patch_version)?.patch_version ?? "—"} rules · position from replay patch ${RUNE_SPOT_PROVENANCE.patch}`}>
          <span className="sm-countdown-anchor" aria-hidden="true" />
          <span className="sm-countdown" data-placement={markerPlacement(spot.x, spot.y)}>
            {spot.labels.map(label => (
              <span key={label.objective} className="sm-countdown-entry" data-objective={label.objective} data-mode={label.mode}>
                <span className="sm-countdown-mark" aria-hidden="true" />
                <span className="sm-countdown-name">{objectiveShortName(label.objective, locale)}</span>
                <span className="sm-countdown-text mono">{label.mode === "ready" ? (locale === "en" ? "Ready" : label.text) : clock(label.remaining)}</span>
              </span>
            ))}
          </span>
        </li>
      ))}
    </ul>
  );
}
