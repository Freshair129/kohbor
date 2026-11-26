import { useEffect, useState } from "react";
import { COUNTDOWN_OBJECTIVES, countdownLabels, deriveRoshanPitStatus, objectiveShortName, runeIconKey, ROSHAN_PIT_RULE, ROSHAN_PIT_SPOTS, RUNE_SPOT_PROVENANCE, spotLabels, type CountdownLabel, type CountdownObjective, type ObjectiveSchedule } from "./objectiveCountdownModel";
import { useObjectiveReadModel } from "./useObjectiveReadModel";
import { MapRoshanStatus } from "./MapRoshanStatus";

const runeIconFiles = import.meta.glob<string>("../assets/dota/6993/runes/*.png", {
  eager: true,
  query: "?url",
  import: "default",
});
const RUNE_ASSET_NAMES: Record<CountdownObjective, string> = {
  power_rune: "power",
  bounty_rune: "bounty",
  wisdom_rune: "wisdom",
  lotus: "lotus",
};
const runeAvailable = Object.fromEntries(COUNTDOWN_OBJECTIVES.map(objective => [
  objective,
  typeof runeIconFiles[`../assets/dota/6993/runes/${RUNE_ASSET_NAMES[objective]}.png`] === "string",
])) as Record<CountdownObjective, boolean>;

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

function markerPlacement(x: number, y: number): "upper-left" | "upper-right" | "lower-left" | "lower-right" {
  if (y < 0.35 && x < 0.5) return "upper-left";
  if (y > 0.65 && x > 0.5) return "lower-right";
  return x < 0.5 ? "upper-right" : "lower-left";
}

function RuneIcon({ label }: { label: CountdownLabel }) {
  const [failed, setFailed] = useState(false);
  const iconKey = runeIconKey(label.objective, runeAvailable);
  const assetName = iconKey === "neutral" ? null : RUNE_ASSET_NAMES[label.objective];
  const src = assetName ? runeIconFiles[`../assets/dota/6993/runes/${assetName}.png`] : null;
  return (
    <span className="sm-countdown-entry" data-objective={label.objective} data-mode={label.mode} aria-hidden="true">
      {src && !failed
        ? <img className="sm-countdown-icon" src={src} alt="" onError={() => setFailed(true)} />
        : <span className="sm-countdown-fallback">●</span>}
      {label.text !== null && <span className="sm-countdown-text">{label.text}</span>}
    </span>
  );
}

function labelDescription(label: CountdownLabel, locale: "th" | "en"): string {
  const name = objectiveShortName(label.objective, locale);
  if (label.mode === "spawned") return `${name} · ${locale === "th" ? "เกิดแล้ว" : "spawned"}`;
  return `${name} · ${label.text ?? "—"}`;
}

/** Game rune icons carry the timer; names and provenance remain available to assistive technology and hover text. */
export function MapCountdowns({ schedule, receivedAtMs, locale, gameClockSeconds = null }: {
  schedule: ObjectiveSchedule | null; receivedAtMs: number | null; locale: "th" | "en"; gameClockSeconds?: number | null;
}) {
  const roshanRead = useObjectiveReadModel(true);
  const now = useNow(schedule !== null || roshanRead !== null);
  const clockSchedule = schedule && Number.isSafeInteger(gameClockSeconds) && gameClockSeconds !== null
    ? { ...schedule, observed_clock_seconds: gameClockSeconds } : schedule;
  const labels = countdownLabels(clockSchedule, receivedAtMs, now);
  const spots = spotLabels(labels);
  const roshanFresh = roshanRead !== null && roshanRead.model.availability === "fresh"
    && now >= roshanRead.receivedAtMs && now - roshanRead.receivedAtMs <= 5_000
    && roshanRead.model.map.clock_time.availability === "fresh"
    && roshanRead.model.map.daytime.availability === "fresh";
  const roshanStatus = roshanFresh
    ? deriveRoshanPitStatus(roshanRead.model.map.clock_time.value, roshanRead.model.map.daytime.value, ROSHAN_PIT_RULE)
    : null;
  return (
    <div className="sm-countdown-layer">
      {spots.length > 0 && <ul className="sm-countdowns" aria-label={locale === "th" ? "ตัวนับเวลารูน" : "Rune timers"}>
        {spots.map(spot => {
          const positionPatch = RUNE_SPOT_PROVENANCE.patch;
          const patch = spot.labels.find(label => label.patch_version)?.patch_version ?? "—";
          const names = spot.labels.map(label => labelDescription(label, locale)).join(", ");
          const tooltip = locale === "th"
            ? `${names} · เวลาเกิดตามกฎ ALERT-008 แพตช์ ${patch} · ตำแหน่งจากรีเพลย์แพตช์ ${positionPatch} (ไม่ทราบ build เกม)`
            : `${names} · Spawn timing from ALERT-008 patch ${patch} · position from replay patch ${positionPatch} (game build unknown)`;
          return <li key={spot.key} className="sm-countdown-spot" style={{ left: (spot.x * 100) + "%", top: (spot.y * 100) + "%" }}
            data-position-patch-differs={spot.positionPatchDiffers || undefined}
            aria-label={names} title={tooltip}>
            <span className="sm-countdown-anchor" aria-hidden="true" />
            <span className="sm-countdown" data-placement={markerPlacement(spot.x, spot.y)} aria-hidden="true">
              {spot.labels.map(label => <RuneIcon key={label.objective} label={label} />)}
            </span>
          </li>;
        })}
      </ul>}
      {roshanStatus && <MapRoshanStatus status={roshanStatus} spots={ROSHAN_PIT_SPOTS} locale={locale}
        comparisonPatch={schedule?.rule_source === "ALERT-008" ? schedule.patch_version : null} />}
    </div>
  );
}
