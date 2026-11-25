import { useEffect, useState } from "react";
import { COUNTDOWN_OBJECTIVES, countdownLabels, objectiveShortName, RUNE_SPOT_PROVENANCE, spotLabels, type CountdownObjective, type ObjectiveSchedule } from "./objectiveCountdownModel";

const RUNE_MARK: Record<CountdownObjective, string> = {
  power_rune: "●",
  bounty_rune: "◆",
  wisdom_rune: "▲",
  lotus: "✿",
};

/** Time to spawn as m:ss; the model keeps whole seconds. */
function clock(seconds: number): string {
  const s = Math.max(0, Math.round(seconds));
  return Math.floor(s / 60) + ":" + String(s % 60).padStart(2, "0");
}

function markerPlacement(x: number, y: number): "upper-left" | "upper-right" | "lower-left" | "lower-right" {
  if (y < 0.35 && x < 0.5) return "upper-left";
  if (y > 0.65 && x > 0.5) return "lower-right";
  return x < 0.5 ? "upper-right" : "lower-left";
}

/** Re-renders countdowns each second while the map is visible. */
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
  const labels = countdownLabels(clockSchedule, receivedAtMs, now);
  const spots = spotLabels(labels);
  const byObjective = new Map(labels.map(label => [label.objective, label]));
  return (
    <div className="sm-countdown-layer">
      {spots.length > 0 && <ul className="sm-countdowns" aria-label={locale === "th" ? "นับถอยหลังรูน" : "Rune countdowns"}>
        {spots.map(spot => (
          <li key={spot.key} className="sm-countdown-spot" style={{ left: (spot.x * 100) + "%", top: (spot.y * 100) + "%" }}
            data-position-patch-differs={spot.positionPatchDiffers || undefined}
            aria-label={spot.labels.map(label => objectiveShortName(label.objective, locale) + " " + (label.mode === "ready" ? (locale === "en" ? "Ready" : "พร้อม") : clock(label.remaining))).join(", ")}
            title={locale === "th"
              ? "เวลาเกิดตามกติกาแพตช์ " + (spot.labels.find(label => label.patch_version)?.patch_version ?? "—") + " · ตำแหน่งจากรีเพลย์แพตช์ " + RUNE_SPOT_PROVENANCE.patch
              : "Spawn timing from patch " + (spot.labels.find(label => label.patch_version)?.patch_version ?? "—") + " rules · position from replay patch " + RUNE_SPOT_PROVENANCE.patch}>
            <span className="sm-countdown-anchor" aria-hidden="true" />
            <span className="sm-countdown" data-placement={markerPlacement(spot.x, spot.y)}>
              {spot.labels.map(label => (
                <span key={label.objective} className="sm-countdown-entry" data-objective={label.objective}
                  data-mode={label.mode} data-full={label.fullChip}>
                  <span className="sm-countdown-mark" aria-hidden="true">{RUNE_MARK[label.objective]}</span>
                  {label.mode === "ready"
                    ? <span className="sm-countdown-text">{locale === "en" ? "Ready" : "พร้อม"}</span>
                    : <>
                      {label.fullChip && <span className="sm-countdown-name">{objectiveShortName(label.objective, locale)}</span>}
                      <span className="sm-countdown-text mono">{clock(label.remaining)}</span>
                    </>}
                </span>
              ))}
            </span>
          </li>
        ))}
      </ul>}
      <ul className="sm-countdown-legend" aria-label={locale === "th" ? "คำอธิบายสัญลักษณ์รูน" : "Rune legend"}>
        {COUNTDOWN_OBJECTIVES.map(objective => {
          const label = byObjective.get(objective);
          const time = label ? label.mode === "ready" ? (locale === "en" ? "Ready" : "พร้อม") : clock(label.remaining) : null;
          return <li key={objective} data-objective={objective}>
            <span className="sm-countdown-mark" aria-hidden="true">{RUNE_MARK[objective]}</span>
            <span>{objectiveShortName(objective, locale)}</span>
            {time && <strong>{time}</strong>}
          </li>;
        })}
      </ul>
    </div>
  );
}
