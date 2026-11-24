import type { ReactNode } from "react";
import { liveCopy, type LiveLocale } from "./copy";
import type { LiveMatchSample, HpPoint, TimelineScoreSample } from "./pinnedPanelsModel";
import {
  buildDamageLog,
  buildFarmSeries,
  buildHpSparkline,
  buildKillTimeline,
  deriveObjectiveTimerRows,
  pinnedPanelLayout,
  type PanelId,
} from "./pinnedPanelsModel";
import { formatGameClock } from "./scoreboardModel";
import { AgentOrb } from "./AgentOrb";
import type { ObjectiveAvailability, ObjectiveMapFields, ObjectiveSchedule } from "./objectiveCountdownModel";
import type { Rect } from "./secondMonitorLayout";

type PanelCopy = ReturnType<typeof liveCopy>["pinnedPanels"];

function pathFor(points: readonly { gameSeconds: number; value: number | null }[], min: number, max: number, width: number, height: number): string {
  const known = points.filter((point): point is { gameSeconds: number; value: number } => point.value !== null);
  if (known.length === 0) return "";
  const first = known[0].gameSeconds;
  const span = Math.max(1, known.at(-1)!.gameSeconds - first);
  const range = Math.max(1, max - min);
  return known.map((point, index) => {
    const x = ((point.gameSeconds - first) / span) * width;
    const y = height - ((point.value - min) / range) * height;
    return `${index === 0 ? "M" : "L"}${x.toFixed(1)},${y.toFixed(1)}`;
  }).join(" ");
}

function sparkline(points: readonly HpPoint[]) {
  const values = points.map(point => point.hp);
  const min = Math.min(...values), max = Math.max(...values);
  return pathFor(points.map(point => ({ gameSeconds: point.gameSeconds, value: point.hp })), min, max, 300, 38);
}

function DamagePanel({ samples, copy }: { samples: readonly LiveMatchSample[]; copy: PanelCopy }) {
  const latestClock = [...samples].reverse().find(sample => sample.gameSeconds !== null)?.gameSeconds ?? null;
  const rows = buildDamageLog(samples);
  const points = buildHpSparkline(samples, latestClock);
  return (
    <section className="pp-panel pp-damage-panel" data-panel="damage" aria-label={`${copy.damageTitle} · ${copy.damageSubtitle}`}>
      <h2>{copy.damageTitle}</h2>
      <p className="pp-subtitle">{copy.damageSubtitle}</p>
      <svg className="pp-hp-chart" viewBox="0 0 300 38" preserveAspectRatio="none" role="img" aria-label={copy.hpSparkline}>
        {points.length > 0 && <path d={sparkline(points)} />}
      </svg>
      <ol className="pp-damage-list">
        {rows.slice(0, 4).map((row, index) => (
          <li key={`${row.gameSeconds ?? "unknown"}-${row.hpAfter}-${index}`}>
            <time className="mono">{formatGameClock(row.gameSeconds)}</time>
            <span>−{row.hpLost} HP</span>
            <span>{copy.hpAfter} {row.hpAfter}</span>
          </li>
        ))}
      </ol>
      {rows.length === 0 && <p className="pp-empty">{copy.noDamage}</p>}
      {rows.length > 4 && <p className="pp-more">{copy.moreDamage(rows.length - 4)}</p>}
    </section>
  );
}

function FarmPanel({ samples, copy }: { samples: readonly LiveMatchSample[]; copy: PanelCopy }) {
  const series = buildFarmSeries(samples);
  const points = series?.points ?? [];
  const draw = (key: "gpm" | "xpm" | "lastHits") => {
    const values = points.flatMap(point => point[key] === null ? [] : [point[key] as number]);
    return pathFor(points.map(point => ({ gameSeconds: point.gameSeconds, value: point[key] })),
      values.length ? Math.min(...values) : 0, values.length ? Math.max(...values) : 1, 300, 22);
  };
  const latest = series?.latest ?? { gpm: null, xpm: null, lastHits: null };
  const ten = series?.lhAt10;
  const xAtTen = ten && points.length > 1
    ? (Math.max(0, 600 - points[0].gameSeconds) / Math.max(1, points.at(-1)!.gameSeconds - points[0].gameSeconds)) * 300
    : null;
  return (
    <section className="pp-panel pp-farm-panel" data-panel="farm" aria-label={copy.farmTitle}>
      <h2>{copy.farmTitle}</h2>
      <div className="pp-farm-values mono">
        <span>GPM {latest.gpm ?? "—"}</span><span>XPM {latest.xpm ?? "—"}</span><span>LH {latest.lastHits ?? "—"}</span>
      </div>
      <svg className="pp-farm-chart" viewBox="0 0 300 72" preserveAspectRatio="none" role="img" aria-label={copy.farmGraph}>
        <line className="pp-chart-separator" x1="0" x2="300" y1="24" y2="24" />
        <line className="pp-chart-separator" x1="0" x2="300" y1="48" y2="48" />
        <path className="pp-series-gpm" d={draw("gpm")} />
        <path className="pp-series-xpm" d={draw("xpm")} transform="translate(0 24)" />
        <path className="pp-series-lh" d={draw("lastHits")} transform="translate(0 48)" />
        {xAtTen !== null && <><line className="pp-lh10-line" x1={xAtTen} x2={xAtTen} y1="0" y2="72" /><text className="pp-lh10-label" x={Math.min(260, xAtTen + 3)} y="12">LH@10 {ten?.lastHits}</text></>}
      </svg>
      <p className="pp-chart-legend"><i className="pp-key-gpm" />GPM <i className="pp-key-xpm" />XPM <i className="pp-key-lh" />LH</p>
    </section>
  );
}

function KillTimelinePanel({ samples, copy }: { samples: readonly TimelineScoreSample[]; copy: PanelCopy }) {
  const points = buildKillTimeline(samples);
  const max = Math.max(1, ...points.flatMap(point => [point.radiant, point.dire]));
  const line = (key: "radiant" | "dire") => pathFor(points.map(point => ({ gameSeconds: point.gameSeconds, value: point[key] })), 0, max, 300, 76);
  const latest = points.at(-1);
  return (
    <section className="pp-panel pp-kills-panel" data-panel="kills" aria-label={copy.killTitle}>
      <h2>{copy.killTitle}</h2>
      <p className="pp-kill-values mono"><span>{copy.radiant} {latest?.radiant ?? "—"}</span><span>{copy.dire} {latest?.dire ?? "—"}</span></p>
      <svg className="pp-kill-chart" viewBox="0 0 300 76" preserveAspectRatio="none" role="img" aria-label={copy.killGraph}>
        <path className="pp-series-radiant" d={line("radiant")} />
        <path className="pp-series-dire" d={line("dire")} />
      </svg>
    </section>
  );
}

function ObjectivePanel({ schedule, receivedAtMs, gameClockSeconds, mapFields, sourceAvailability, copy, locale }: {
  schedule: ObjectiveSchedule | null; receivedAtMs: number | null; gameClockSeconds: number | null;
  mapFields: ObjectiveMapFields | null; sourceAvailability: ObjectiveAvailability;
  copy: PanelCopy;
  locale: LiveLocale;
}) {
  const rows = deriveObjectiveTimerRows(schedule, receivedAtMs, Date.now(), gameClockSeconds, mapFields, sourceAvailability, locale === "en" ? "en" : "th");
  return (
    <section className="pp-panel pp-objectives-panel" data-panel="objectives" aria-label={copy.objectivesTitle}>
      <h2>{copy.objectivesTitle}</h2>
      <div className="pp-objective-grid">
        {rows.map(row => <div key={row.id} className="pp-objective-row" data-state={row.state}>
          <span>{copy.timerNames[row.id]}</span>
          <small className="pp-objective-patch">{row.patchVersion ? `${locale === "en" ? "Rule" : "กติกา"} ${row.patchVersion}` : ""}</small>
          <strong className="mono">{row.state === "unknown" ? copy.unknown : row.state === "ready" ? copy.ready : row.text}</strong>
        </div>)}
      </div>
    </section>
  );
}

export function PinnedPanels({ box, samples, scoreSamples, schedule, scheduleReceivedAtMs, mapFields, sourceAvailability, gameClockSeconds, locale }: {
  box: Rect;
  samples: readonly LiveMatchSample[];
  scoreSamples: readonly TimelineScoreSample[];
  schedule: ObjectiveSchedule | null;
  scheduleReceivedAtMs: number | null;
  mapFields: ObjectiveMapFields | null;
  sourceAvailability: ObjectiveAvailability;
  gameClockSeconds: number | null;
  locale: LiveLocale;
}) {
  const layout = pinnedPanelLayout(box);
  const copy = liveCopy(locale).pinnedPanels;
  const content: Record<PanelId, ReactNode> = {
    objectives: <ObjectivePanel schedule={schedule} receivedAtMs={scheduleReceivedAtMs} gameClockSeconds={gameClockSeconds}
      mapFields={mapFields} sourceAvailability={sourceAvailability} copy={copy} locale={locale} />,
    damage: <DamagePanel samples={samples} copy={copy} />,
    orb: <AgentOrb locale={locale} />,
    farm: <FarmPanel samples={samples} copy={copy} />,
    kills: <KillTimelinePanel samples={scoreSamples} copy={copy} />,
  };
  return <div className="pp-panel-grid" data-panels={layout.panels.map(panel => panel.id).join(",")} data-scroll={layout.scroll}>
    {layout.panels.map(({ id, rect }) => <div key={id} className="pp-panel-slot" style={{ left: rect.x, top: rect.y, width: rect.w, height: rect.h }}>
      {content[id]}
    </div>)}
  </div>;
}
