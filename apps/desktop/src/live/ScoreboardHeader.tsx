import type { OwnSnapshot } from "../types";
import { formatGameClock, formatScore, killMomentum, type ScoreSample } from "./scoreboardModel";

function fieldValue(field: { value: number | null; availability: string } | undefined): number | null {
  return field?.availability === "fresh" && typeof field.value === "number" && Number.isSafeInteger(field.value) ? field.value : null;
}

export function ScoreboardHeader({ snapshot, phase, locale, scoreSamples = [] }: {
  snapshot: OwnSnapshot | null;
  phase: string | null;
  locale: "th" | "en";
  scoreSamples?: readonly ScoreSample[];
}) {
  const map = snapshot?.map;
  const radiant = fieldValue(map?.radiant_score);
  const dire = fieldValue(map?.dire_score);
  const clock = fieldValue(map?.clock_time);
  const momentum = killMomentum(phase === "live" ? scoreSamples : [], performance.now());
  const label = locale === "th" ? "โมเมนตัมคิล (3 นาทีล่าสุด)" : "Kill momentum (last 3 min)";
  const note = locale === "th" ? "แสดงจำนวนคิล ไม่ใช่โอกาสชนะ" : "Shows kills, not win probability";
  const direction = momentum?.leaning === "radiant" ? (locale === "th" ? "ฝั่ง Radiant นำ" : "Radiant ahead")
    : momentum?.leaning === "dire" ? (locale === "th" ? "ฝั่ง Dire นำ" : "Dire ahead")
      : (locale === "th" ? "เป็นกลางหรือข้อมูลยังไม่พอ" : "Neutral or not enough data");

  return (
    <div className="live-scoreboard" role="group" aria-label={locale === "th" ? "สกอร์บอร์ดแมตช์" : "Match scoreboard"}>
      <div className="live-scoreboard-row">
        <div className="live-scoreboard-team live-scoreboard-radiant">
          <span>{locale === "th" ? "Radiant" : "Radiant"}</span>
          <strong className="mono">{formatScore(radiant)}</strong>
        </div>
        <time className="live-scoreboard-clock mono" aria-label={locale === "th" ? "เวลาในเกม" : "Game clock"}>
          {formatGameClock(clock)}
        </time>
        <div className="live-scoreboard-team live-scoreboard-dire">
          <strong className="mono">{formatScore(dire)}</strong>
          <span>Dire</span>
        </div>
      </div>
      <div className="live-momentum" title={note} aria-label={`${label}: ${direction}`}>
        <div className="live-momentum-label">
          <span>{label}</span>
          <small>{note}</small>
        </div>
        <div className="live-momentum-track" role="meter" aria-label={label} aria-valuemin={0} aria-valuemax={100}
          aria-valuenow={momentum?.radiantPercent ?? undefined} aria-valuetext={direction} data-empty={momentum?.radiantPercent === null || momentum === null}>
          {momentum?.radiantPercent !== null && momentum && <>
            <span className="live-momentum-radiant" style={{ width: `${momentum.radiantPercent}%` }} />
            <span className="live-momentum-dire" style={{ width: `${100 - momentum.radiantPercent}%` }} />
          </>}
        </div>
      </div>
    </div>
  );
}
