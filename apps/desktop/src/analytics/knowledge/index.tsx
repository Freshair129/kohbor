import type { ReactNode } from "react";
import { ANALYTICS_TABS } from "../analyticsModel";
import { ANALYTICS_OPPONENTS_COPY } from "../../copy";
import { buildLineSegments, heroComparisonEngineMetric, laneBadgeLabel, laneExpectationReading, matchupRows, showLaneExpectationInStrip } from "./presentationModel.mjs";
import { buildingMapPoint, buildMatchSummaryViewModel } from "./matchSummaryModel.mjs";
import { buildOpponentsViewModel } from "./opponentsModel.mjs";
import { BUNDLED, replayMapUrlForPatch } from "../../replay/bundledIcons";
import { gameModeName, iconResolver } from "../../replay/matchBundle";
import { displayHeroName } from "../../deck/heroName";
import type { MapCalibration } from "../../replay/replayPack.mjs";
import { HeroBenchmarksPanel } from "./HeroBenchmarksPanel";
import "./knowledge.css";

export type KnowledgeHero = {
  id: string;
  hero: string;
  side: "radiant" | "dire";
  slot: number;
  lane?: { lane: "top" | "mid" | "bot" | null; share?: number | null; reason?: string };
  final?: Record<string, { value?: number | null; reason?: string } | number | null>;
  rates?: { gpm?: number | null; gpm_reason?: string | null; gpm_source?: string | null; xpm?: number | null };
  gold_earned?: number | null;
  gold_earned_reason?: string | null;
  gold_earned_source?: string | null;
  hero_damage?: number | null;
  hero_damage_reason?: string | null;
  hero_damage_source?: string | null;
  phases?: Record<string, Record<string, unknown>>;
  items?: unknown;
  final_items?: unknown;
};


export type KnowledgeDigest = {
  meta?: Record<string, any>;
  heroes?: KnowledgeHero[];
  lanes?: Array<Record<string, any>>;
  team_phases?: Record<string, any>;
  teamfights?: Array<Record<string, any>>;
  objectives?: Record<string, any>;
  deaths?: Array<Record<string, any>>;
  advantage_per_min?: Array<Record<string, any>> | null;
  advantage_per_min_reason?: string | null;
  provisional_thresholds?: { label?: string };
  opponent_stats?: Record<string, unknown> | null;
};

export type SavedReport = {
  id: string;
  meta: Record<string, any>;
  stale_reasons?: string[];
  markdown?: string;
};

/**
 * Props shared by the exported knowledge panels. `digest` is the `digest` field
 * from `analytics_knowledge_get`; the host owns data loading, navigation and AI
 * command calls. Time links call `onSeek` and report buttons only call their
 * click handlers, so mounting a panel never starts a provider request.
 */
export interface KnowledgePanelProps {
  /** Locally computed K0-K2 digest, or null while unavailable/loading. */
  digest: KnowledgeDigest | null;
  /** Show a loading state in the summary tab. */
  loading?: boolean;
  /** Selected roster hero and optional comparison opponent. */
  selectedHeroId?: string | null;
  opponentHeroId?: string | null;
  /** Update the selected hero IDs in the host page. */
  onSelectHero?: (heroId: string) => void;
  onSelectOpponentHero?: (heroId: string) => void;
  /** Seek the replay scrubber to a game-clock time in seconds. */
  onSeek?: (seconds: number) => void;
  /** Locally selected replay map calibration for the destroyed-building overlay. */
  mapCalibration?: MapCalibration | null;
  /** Match-local own hero identity and raw validated match section for death attribution. */
  ownHeroId?: string | null;
  matchData?: Record<string, any> | null;
  /** Persisted report metadata; load markdown through the host read command. */
  reports?: SavedReport[];
  selectedReportId?: string | null;
  onSelectReport?: (reportId: string) => void;
  /** Explicit user action handlers; wire to analytics_report_generate. */
  onGenerateReport?: () => void;
  onRegenerateReport?: () => void;
  generatingReport?: boolean;
  reportDisabled?: boolean;
  /** Current provider cost note; do not quote unverified pricing. */
  costEstimateText?: string;
  /** Host-owned safe Markdown renderer, including #doc and time links. */
  renderMarkdown?: (markdown: string) => ReactNode;
}

const LANE_NAMES: Record<string, string> = { top: "เลนบน", mid: "เลนกลาง", bot: "เลนล่าง", unknown: "ไม่ทราบเลน" };
const SIDE_NAMES: Record<string, string> = { radiant: "Radiant", dire: "Dire" };
const valueAt = (value: any) => typeof value === "number" || typeof value === "string" ? String(value) : "— ไม่ทราบ";
const timeText = (seconds: unknown) => {
  if (typeof seconds !== "number" || !Number.isFinite(seconds) || seconds < 0) return "— ไม่ทราบ";
  const total = Math.floor(seconds);
  return `${String(Math.floor(total / 60)).padStart(2, "0")}:${String(total % 60).padStart(2, "0")}`;
};
const profileLane = (hero: KnowledgeHero) => hero.lane?.lane ?? null;
const phaseLabel = (phase: string) => ({ laning: "ช่วงยืนเลน", mid: "ช่วงกลางเกม", late: "ช่วงท้ายเกม" }[phase] ?? "ช่วงที่ไม่ทราบ");
const objectiveIcon = (group: string) => group === "buildings" ? "▣" : group === "wards" ? "◉" : "◆";
const localSummaryIcon = iconResolver(BUNDLED.icons);

function itemRows(value: unknown): Record<string, unknown>[] {
  const rows = Array.isArray(value) ? value : value && typeof value === "object" ? Object.values(value as Record<string, unknown>) : [];
  return rows.flatMap((row) => {
    if (row && typeof row === "object" && !Array.isArray(row)) return [row as Record<string, unknown>];
    if (Array.isArray(row)) return row.filter((entry): entry is Record<string, unknown> => !!entry && typeof entry === "object" && !Array.isArray(entry));
    return [];
  });
}

function ItemRows({ value, emptyLabel = "— ไม่ทราบ" }: { value: unknown; emptyLabel?: string }) {
  const rows = itemRows(value);
  if (!rows.length) return <span>{emptyLabel}</span>;
  return <ul className="a2k-item-rows">{rows.map((item, index) => {
    const rawName = [item.name_th, item.display_name, item.name].find((name) => typeof name === "string" && !/^(npc_|item_)/i.test(name)) as string | undefined;
    const time = item.time_s ?? item.acquired_at_s ?? item.t;
    return <li key={index}><span aria-hidden="true">◈</span><span>{rawName ?? "ไอเท็ม"}</span><time>{timeText(time)}</time></li>;
  })}</ul>;
}

/** SVG chart for the local digest. Unknown samples split the line into separate segments. */
export function AdvantageChart({ digest, onSeek }: Pick<KnowledgePanelProps, "digest" | "onSeek">) {
  const rows = digest?.advantage_per_min ?? [];
  const width = 560;
  const height = 190;
  const pad = 18;
  const series = [
    { key: "radiant_nw_diff", label: "ส่วนต่างมูลค่าสุทธิ", color: "var(--ally)" },
    { key: "radiant_xp_diff", label: "ส่วนต่างประสบการณ์", color: "var(--warning)" },
  ];
  const points = series.map(({ key }) => rows.map((row) => typeof row[key] === "number" ? row[key] as number : null));
  const finite = points.flat().filter((value): value is number => value !== null && Number.isFinite(value));
  const noAdvantageSamples = finite.length === 0;
  const domain: [number, number] = finite.length ? [Math.min(...finite), Math.max(...finite)] : [0, 1];
  const zeroY = !noAdvantageSamples && domain[0] <= 0 && domain[1] >= 0 && domain[0] !== domain[1]
    ? pad + ((domain[1] - 0) * (height - pad * 2)) / (domain[1] - domain[0])
    : height / 2;
  const duration = typeof digest?.meta?.duration_s === "number" ? digest.meta.duration_s : 0;
  const sideByHero = new Map((digest?.heroes ?? []).flatMap((hero) => hero.id && (hero.side === "radiant" || hero.side === "dire") ? [[hero.id, hero.side] as const] : []));
  const killMarkers = (digest?.deaths ?? []).flatMap((death) => {
    const side = typeof death.killer === "string" ? sideByHero.get(death.killer) : null;
    return side && duration > 0 && typeof death.time_s === "number" && death.time_s >= 0 && death.time_s <= duration
      ? [{ eventRef: String(death.event_ref ?? `${side}-${death.time_s}`), side, time: death.time_s }]
      : [];
  });
  const axisValue = (value: number) => `${value > 0 ? "+" : ""}${Math.round(value).toLocaleString("th-TH")}`;
  return <section className="a2k-chart" aria-label="กราฟความได้เปรียบตามเวลา">
    <div className="a2k-chart__legend">{series.map((item) => <span key={item.key}><i style={{ background: item.color }} />{item.label}</span>)}</div>
    {noAdvantageSamples && <p>— ไม่ทราบ · {digest?.advantage_per_min_reason ?? "ไม่มีตัวอย่างความได้เปรียบใน digest"}</p>}
    <div className="a2k-chart__sides"><span>Radiant นำ</span><span>Dire นำ</span></div>
    <svg viewBox={`0 0 ${width} ${height}`} role="group" aria-label="กราฟส่วนต่าง Radiant ต่อ Dire">
      {!noAdvantageSamples && <>
        <text className="a2k-chart__axis-title" transform={`translate(7 ${height / 2}) rotate(-90)`} textAnchor="middle">ส่วนต่าง Radiant − Dire</text>
        <text className="a2k-chart__y-label" x={pad} y={pad + 4}>{axisValue(domain[1])}</text>
        <text className="a2k-chart__y-label" x={pad} y={height / 2}>{axisValue((domain[0] + domain[1]) / 2)}</text>
        <text className="a2k-chart__y-label" x={pad} y={height - pad}>{axisValue(domain[0])}</text>
        <line x1={pad} x2={width - pad} y1={zeroY} y2={zeroY} className="a2k-chart__zero" />
      </>}
      {series.map((item, seriesIndex) => buildLineSegments(points[seriesIndex] ?? [], width - pad * 2, height - pad * 2, domain).map((segment, index) => {
        const coords = segment.map(({ x, y }) => `${x + pad},${y + pad}`).join(" ");
        return <polyline key={`${item.key}-${index}`} points={coords} fill="none" stroke={item.color} strokeWidth="3" strokeLinejoin="round" strokeLinecap="round" />;
      }))}
      {rows.map((row) => typeof row.minute === "number" && <rect key={`seek-${row.minute}`} x={pad + (row.minute * (width - pad * 2)) / Math.max(rows.length - 1, 1) - 4} y={pad} width="8" height={height - pad * 2} fill="transparent" role="button" tabIndex={0} aria-label={`ไปนาที ${row.minute}`} onClick={() => onSeek?.(row.minute * 60)} onKeyDown={(event) => { if (event.key === "Enter" || event.key === " ") onSeek?.(row.minute * 60); }} />)}
      {killMarkers.map((marker) => <g key={marker.eventRef} role="button" tabIndex={0} aria-label={`${marker.side === "radiant" ? "Radiant" : "Dire"} สังหารที่ ${timeText(marker.time)}`} onClick={() => onSeek?.(marker.time)} onKeyDown={(event) => { if (event.key === "Enter" || event.key === " ") onSeek?.(marker.time); }}>
        <circle cx={pad + (marker.time / duration) * (width - pad * 2)} cy={zeroY} r="5" fill={marker.side === "radiant" ? "var(--ally)" : "var(--enemy)"} stroke="var(--surface)" strokeWidth="2" />
        <title>{marker.side === "radiant" ? "Radiant" : "Dire"} · {timeText(marker.time)}</title>
      </g>)}
    </svg>
    <div className="a2k-chart__axis"><span>เวลาในแมตช์</span><span>0:00 – {timeText(digest?.meta?.duration_s)}</span></div>
    <small>ช่วงที่ไม่มีตัวเลขหมายถึงรีเพลย์ไม่มีข้อมูลในเวลานั้น</small>
  </section>;
}

/** The three lane cards display the digest outcome and its provisional basis. */
export function LaneCards({ digest, onSeek, compact = false, compactLimit = 2 }: Pick<KnowledgePanelProps, "digest" | "onSeek"> & { compact?: boolean; compactLimit?: number }) {
  const lanes = digest?.lanes ?? [];
  if (compact) return <div className="a2k-postgame-lane-list">
    {(["top", "mid", "bot"] as const).slice(0, Math.max(1, Math.min(3, compactLimit))).map((lane) => {
      const row = lanes.find((item) => item.lane === lane);
      const boundary = typeof row?.boundary_s === "number" ? row.boundary_s : null;
      const outcome = laneBadgeLabel(row?.outcome ?? null);
      const reading = boundary !== null ? `ไป ${timeText(boundary)}` : `NW ${valueAt(row?.nw_diff_10)}`;
      const lineTitle = [LANE_NAMES[lane], outcome, reading, row?.outcome_reason].filter(Boolean).join(" · ");
      return <div className="a2k-postgame-detail-item" key={lane} title={lineTitle}>
        <strong title={LANE_NAMES[lane]}>{LANE_NAMES[lane]}</strong><span title={outcome}>{outcome}</span>
        {boundary !== null ? <button type="button" title={lineTitle} onClick={() => onSeek?.(boundary)}>{reading}</button> : <span title={reading}>{reading}</span>}
      </div>;
    })}
    {3 > Math.max(1, Math.min(3, compactLimit)) && <span className="a2k-postgame-more">+{3 - Math.max(1, Math.min(3, compactLimit))} เลน</span>}
  </div>;
  return <div className="a2k-lanes">{["top", "mid", "bot"].map((lane) => {
    const row = lanes.find((item) => item.lane === lane);
    const radiantReading = laneExpectationReading(row, "radiant");
    const direReading = laneExpectationReading(row, "dire");
    const score = typeof row?.score?.value === "number" ? `${row.score.value >= 0 ? "+" : ""}${row.score.value.toFixed(1)}` : "— ไม่ทราบ";
    const expected = typeof row?.expectation?.value === "number" ? `${row.expectation.value >= 0 ? "+" : ""}${row.expectation.value.toFixed(1)}` : null;
    return <article key={lane} className="a2k-lane-card">
      <header><strong>{LANE_NAMES[lane]}</strong><span>{laneBadgeLabel(row?.outcome ?? null)}</span></header>
      <div className="a2k-lane-card__teams">{(["radiant", "dire"] as const).map((side) => <p key={side}><b>{SIDE_NAMES[side]}</b> {(row?.[side] ?? []).map((hero: any) => hero.name ?? hero.hero).join(" · ") || "— ไม่ทราบ"}</p>)}</div>
      <dl><dt>มูลค่าสุทธิต่างกัน ณ 10:00</dt><dd>{valueAt(row?.nw_diff_10)}</dd><dt>ประสบการณ์ต่างกัน ณ 10:00</dt><dd>{valueAt(row?.xp_diff_10)}</dd></dl>
      <section className="a2k-lane-reading" aria-label="คะแนนเลนเทียบค่าคาดหมาย">
        <header><strong>คะแนนเลนของเกาะเบาะ</strong><span>ทดลอง · เกณฑ์ชั่วคราว</span></header>
        <dl><dt>คะแนนจริง · มุม Radiant</dt><dd>{score}</dd>
          <dt>ค่าคาดหมาย</dt><dd>{expected ?? "ยังไม่มีค่าคาดหมาย"}</dd>
          {expected !== null && <><dt>แหล่งค่าคาดหมาย</dt><dd>{row?.expectation?.source_label ?? "— ไม่ทราบ"}</dd></>}
          {row?.personal_average && <><dt>ค่าเฉลี่ยของคุณ</dt><dd>{row.personal_average.value >= 0 ? "+" : ""}{row.personal_average.value.toFixed(1)} · จาก {typeof row.personal_average.n === "number" ? `${row.personal_average.n} แมตช์` : "— ไม่ทราบจำนวนแมตช์"}</dd></>}
        </dl>
        {expected === null ? <p className="a2k-lane-reading__unknown">ยังไม่มีค่าคาดหมาย</p> : <>
          <p className="a2k-lane-reading__verdict">Radiant · {radiantReading.verdict ?? "— ไม่ทราบ"} · {radiantReading.trialLabel}</p>
          <p className="a2k-lane-reading__verdict">Dire · {direReading.verdict ?? "— ไม่ทราบ"} · {direReading.trialLabel}</p>
          <small>{radiantReading.explanation}</small>
        </>}
      </section>
      {typeof row?.boundary_s === "number" ? <button type="button" onClick={() => onSeek?.(row.boundary_s)}>ไปที่ {timeText(row.boundary_s)}</button> : <small>เวลาแบ่งเลน: — ไม่ทราบ</small>}
    </article>;
  })}</div>;
}

function BuildingLosses({ digest, mapCalibration, onSeek, mapImageUrl, compact = false, compactLimit = 2 }: Pick<KnowledgePanelProps, "digest" | "mapCalibration" | "onSeek"> & { mapImageUrl?: string | null; compact?: boolean; compactLimit?: number }) {
  const buildings = Array.isArray(digest?.objectives?.buildings) ? digest.objectives.buildings : [];
  const buildingsKnown = Array.isArray(digest?.objectives?.buildings);
  const destroyed = buildings.filter((item: any) => item.destroyed === true);
  const unknownCount = buildings.filter((item: any) => item.destroyed == null).length;
  const mapUrl = mapImageUrl ?? null;
  const positioned = destroyed.flatMap((building: any, index: number) => {
    const point = buildingMapPoint(building.position, mapCalibration);
    return point ? [{ building, index, point }] : [];
  });
  const showMap = !!mapUrl && !!mapCalibration && positioned.length > 0;
  const mapReason = !mapUrl
    ? "ไม่มีภาพมินิแมปในไอคอนที่ติดตั้งในเครื่อง"
    : !mapCalibration
      ? "ไม่มี calibration ของ replay สำหรับแปลงพิกัดโลกเป็นตำแหน่งบนแผนที่"
      : "ไม่มีตำแหน่งอาคารที่แปลงได้ด้วย calibration ของแผนที่";
  if (compact) return <section className="a2k-building-losses a2k-building-losses--compact" aria-label="อาคารที่ถูกทำลาย">
    <h3>อาคาร</h3>
    {destroyed.length > 0 ? <>
      <ul className="a2k-building-losses__list">{destroyed.slice(0, compactLimit).map((building: any, index: number) => {
        const team = building.team === "radiant" ? "Radiant" : building.team === "dire" ? "Dire" : "ทีมไม่ทราบ";
        const name = typeof building.name === "string" ? building.name : "อาคารไม่ทราบชื่อ";
        const at = timeText(building.destroyed_at_s);
        const buildingTitle = `${team} · ${name} · ${at}`;
        return <li key={`${building.name ?? "building"}-${index}`} title={buildingTitle}>
          <span className={`a2k-side-dot is-${building.team === "radiant" || building.team === "dire" ? building.team : "unknown"}`} aria-hidden="true" />
          <span>{team} · {name}</span>
          <button type="button" title={buildingTitle} disabled={typeof building.destroyed_at_s !== "number"} onClick={() => typeof building.destroyed_at_s === "number" && onSeek?.(building.destroyed_at_s)}>{at}</button>
        </li>;
      })}{destroyed.length - Math.min(destroyed.length, compactLimit) + unknownCount > 0 && <li className="a2k-postgame-more" title={`รายการเพิ่มเติม ${destroyed.length - Math.min(destroyed.length, compactLimit)} รายการ · สถานะไม่ทราบ ${unknownCount} รายการ`}>+{destroyed.length - Math.min(destroyed.length, compactLimit) + unknownCount} รายการ</li>}</ul>
    </> : <p className="a2k-building-losses__unknown">{buildingsKnown ? unknownCount > 0 ? `— ไม่ทราบ · อาคาร ${unknownCount} รายการ` : "ไม่พบอาคารที่ถูกทำลาย" : `— ไม่ทราบ · ${digest?.objectives?.buildings_reason ?? mapReason}`}</p>}
  </section>;
  return <section className="a2k-building-losses" aria-label="อาคารที่ถูกทำลาย">
    <h3>อาคารที่ถูกทำลาย</h3>
    {showMap ? <div className="a2k-building-map" role="group" aria-label="มินิแมปตำแหน่งอาคารที่ถูกทำลาย">
      <img src={mapUrl} alt="" aria-hidden="true" />
      {positioned.map(({ building, index, point }: any) => {
        const side = building.team === "radiant" || building.team === "dire" ? building.team : "unknown";
        const at = typeof building.destroyed_at_s === "number" ? building.destroyed_at_s : null;
        const name = typeof building.name === "string" ? building.name : "อาคารไม่ทราบชื่อ";
        return <button key={`${building.name ?? "building"}-${index}`} type="button" className={`a2k-building-map__mark is-${side}`}
          style={{ left: `${point.x}%`, top: `${point.y}%` }} disabled={at === null}
          aria-label={`${side === "unknown" ? "ทีมไม่ทราบ" : side === "radiant" ? "Radiant" : "Dire"} · ${name} · ${timeText(at)}`}
          title={`${side === "unknown" ? "ทีมไม่ทราบ" : side === "radiant" ? "Radiant" : "Dire"} · ${name} · ${timeText(at)}`}
          onClick={() => at !== null && onSeek?.(at)}>▣</button>;
      })}
    </div> : destroyed.length > 0 ? <p className="a2k-building-losses__unknown">— แสดงรายการแทนมินิแมป · {mapReason}</p> : null}
    {showMap && positioned.length < destroyed.length && <small className="a2k-building-losses__unknown">อาคารที่เหลือไม่มีตำแหน่งซึ่งแปลงได้ด้วย calibration นี้ จึงแสดงเป็นรายการ</small>}
    {destroyed.length > 0 ? <ul className="a2k-building-losses__list">{destroyed.map((building: any, index: number) => <li key={`${building.name ?? "building"}-${index}`}>
      <span className={`a2k-side-dot is-${building.team === "radiant" || building.team === "dire" ? building.team : "unknown"}`} aria-hidden="true" />
      <span>{building.team === "radiant" ? "Radiant" : building.team === "dire" ? "Dire" : "ทีมไม่ทราบ"} · {typeof building.name === "string" ? building.name : "อาคารไม่ทราบชื่อ"}</span>
      <button type="button" disabled={typeof building.destroyed_at_s !== "number"} title={building.destroyed_at_s_reason ?? undefined}
        onClick={() => typeof building.destroyed_at_s === "number" && onSeek?.(building.destroyed_at_s)}>{timeText(building.destroyed_at_s)}</button>
    </li>)}</ul> : !buildingsKnown
      ? <p className="a2k-building-losses__unknown">— ไม่ทราบ · {digest?.objectives?.buildings_reason ?? "objective tracks are missing"}</p>
      : unknownCount > 0 ? <p className="a2k-building-losses__unknown">— ไม่ทราบ · สถานะอาคาร {unknownCount} รายการไม่ทราบ</p>
        : <p className="a2k-building-losses__unknown">ไม่พบอาคารที่ถูกทำลายใน digest</p>}
    {unknownCount > 0 && destroyed.length > 0 && <small className="a2k-building-losses__unknown">ไม่ทราบสถานะอาคาร {unknownCount} รายการ · {buildings.find((item: any) => item.destroyed == null)?.destroyed_reason ?? "ไม่มีตัวอย่าง health ที่ใช้ยืนยันสถานะ"}</small>}
  </section>;
}

function TeamfightSummary({ digest, onSeek, compact = false, compactLimit = 2 }: Pick<KnowledgePanelProps, "digest" | "onSeek"> & { compact?: boolean; compactLimit?: number }) {
  const fights = digest?.teamfights ?? [];
  const visible = compact ? fights.slice(0, compactLimit) : fights;
  return <section className={`a2k-summary__teamfights${compact ? " a2k-summary__teamfights--compact" : ""}`}>
    <h3>จังหวะสำคัญ</h3><ul className="a2k-event-rows">{visible.map((fight) => {
      const start = typeof fight.t?.[0] === "number" ? fight.t[0] : null;
      const end = typeof fight.t?.[1] === "number" ? fight.t[1] : null;
      const deaths = Array.isArray(fight.deaths) ? fight.deaths.length : null;
      const timeLabel = start !== null ? `${timeText(start)}–${timeText(end)}` : "— ไม่ทราบ";
      const eventLabel = `ทีมไฟต์ · ตาย ${deaths ?? "— ไม่ทราบ"}`;
      const eventTitle = `${timeLabel} · ${eventLabel}`;
      return <li key={fight.n} title={eventTitle}><span aria-hidden="true">⚔</span>{start !== null ? <button type="button" title={timeLabel} onClick={() => onSeek?.(start)}>{timeLabel}</button> : <span>{timeLabel}</span>}<span title={eventLabel}>{eventLabel}</span></li>;
    })}{compact && fights.length > visible.length && <li className="a2k-postgame-more" title={`มีจังหวะสำคัญเพิ่มเติม ${fights.length - visible.length} รายการ`}>+{fights.length - visible.length} จังหวะ</li>}</ul>
    {compact && fights.length === 0 && <p className="a2k-building-losses__unknown">— ไม่ทราบ · ไม่มีทีมไฟต์ใน digest</p>}
  </section>;
}

/** Compact matchup strip for lane-by-lane hero assignment; unknown positions remain visible. */
export function MatchupStrip({ digest, selectedHeroId, onSelectHero }: Pick<KnowledgePanelProps, "digest" | "selectedHeroId" | "onSelectHero">) {
  const heroes = digest?.heroes ?? [];
  const rows = matchupRows(heroes.map((hero) => ({ ...hero, lane: profileLane(hero) })));
  const visibleRows = rows.filter((row) => row.lane !== "unknown" || row.heroes.length > 0);
  return <div className="a2k-matchups">
    <div className="a2k-matchups__lanes">{visibleRows.map((row) => <section key={row.lane} className="a2k-matchups__lane" aria-label={LANE_NAMES[row.lane]}>
      <strong>{LANE_NAMES[row.lane]}</strong>
      {row.heroes.length === 0 ? <span className="a2k-muted">— ไม่ทราบ</span> : row.heroes.map((hero) => <button key={hero.id} type="button" className={selectedHeroId === hero.id ? "is-selected" : ""} onClick={() => onSelectHero?.(hero.id)} title={`${SIDE_NAMES[hero.side]} · ${hero.hero}`}>
        <i className={`a2k-side-dot is-${hero.side}`} /><span>{hero.hero}</span>
        {showLaneExpectationInStrip(row.heroes, digest?.lanes?.find((item) => item.lane === row.lane)?.constants?.lane_strip_max_cells) && (() => {
          const laneDigest = digest?.lanes?.find((item) => item.lane === row.lane);
          const reading = laneExpectationReading(laneDigest, hero.side);
          return reading.verdict ? <small className="a2k-matchups__expectation" title={reading.explanation ?? undefined}>{reading.verdict} · {reading.trialLabel}</small> : null;
        })()}
      </button>)}</section>)}</div>
    <div className="a2k-matchups__score"><span>Radiant</span><b>{typeof digest?.meta?.kills?.radiant === "number" ? digest.meta.kills.radiant : "—"} : {typeof digest?.meta?.kills?.dire === "number" ? digest.meta.kills.dire : "—"}</b><span>Dire</span></div>
  </div>;
}

/** Combined post-match summary using only fields present in the local digest. */
function OpponentsSection({ digest, ownHeroId, matchData }: Pick<KnowledgePanelProps, "digest" | "ownHeroId" | "matchData">) {
  const copy = ANALYTICS_OPPONENTS_COPY;
  const model = buildOpponentsViewModel({ digest, ownHeroId, matchData });
  const displayName = (head: { state: string; names?: string[] }) => {
    if (head.state === "unknown") return copy.unknown;
    if (head.state === "none") return copy.noEvents;
    const values = head.names ?? [];
    return values.length ? values.join(" / ") : copy.unknown;
  };
  const detail = (head: { state: string; count?: number | null; gold?: number | null }, engineStat = false) => {
    if (head.state === "unknown") return engineStat ? copy.missingEngine : null;
    if (head.gold !== undefined && head.gold !== null) return copy.gold(head.gold);
    return head.count === null || head.count === undefined ? null : copy.count(head.count);
  };
  const tiles = [
    ["mostKills", copy.headlines.mostKills, model.headlines.mostKills, false],
    ["mostKillGold", copy.headlines.mostKillGold, model.headlines.mostKillGold, true],
    ["mostDeaths", copy.headlines.mostDeaths, model.headlines.mostDeaths, false],
    ["mostGoldLost", copy.headlines.mostGoldLost, model.headlines.mostGoldLost, true],
    ["mostJungleGold", copy.headlines.mostJungleGold, model.headlines.mostJungleGold, true],
  ] as const;
  const metric = (value: number | null) => value === null ? "—" : value.toLocaleString("th-TH");
  return <section className="a2k-opponents" aria-label={copy.title}>
    <h3>{copy.title}</h3>
    {model.state === "unknown" && <p className="a2k-opponents__unknown">{model.reason === "enemy_team_unknown" ? copy.emptyEnemyTeam : copy.unknownTeam}</p>}
    <div className="a2k-opponents__headlines">
      {tiles.map(([key, label, head, engineStat]) => <article className="a2k-opponents__tile" key={key}>
        <h4>{label}</h4>
        <strong>{displayName(head)}</strong>
        {detail(head, engineStat) && <small>{detail(head, engineStat)}</small>}
        {key === "mostJungleGold" && model.jungleUnattached !== null && model.jungleUnattached > 0 && <small>{copy.jungleUnattached(model.jungleUnattached)}</small>}
      </article>)}
    </div>
    {model.enemyRows.length > 0 && <div className="a2k-opponents__table-wrap">
      <table><thead><tr><th scope="col">{copy.columns.hero}</th><th scope="col">{copy.columns.kills}</th><th scope="col">{copy.columns.deaths}</th><th scope="col">{copy.columns.killGold}</th><th scope="col">{copy.columns.goldLost}</th></tr></thead>
        <tbody>{model.enemyRows.map(row => <tr key={row.heroId}>
          <th scope="row">{row.hero}</th><td>{metric(row.kills)}</td><td>{metric(row.deaths)}</td><td>{metric(row.killGold)}</td><td>{metric(row.goldLost)}</td>
        </tr>)}</tbody>
      </table>
    </div>}
    <div className="a2k-opponents__other">
      <span>{model.otherDeaths === null ? copy.otherDeathsUnknown : copy.otherDeaths(model.otherDeaths)}</span>
      {model.otherGoldLost !== null && <span>{copy.otherGoldLost(model.otherGoldLost)}</span>}
      {model.unknownKillerDeaths !== null && model.unknownKillerDeaths > 0 && <span>{copy.unknownKillerDeaths(model.unknownKillerDeaths)}</span>}
    </div>
  </section>;
}

export function MatchSummaryTab({ digest, loading, onSeek, mapCalibration, ownHeroId, matchData }: Pick<KnowledgePanelProps, "digest" | "loading" | "onSeek" | "mapCalibration" | "ownHeroId" | "matchData">) {
  if (loading) return <div className="a2k-empty" role="status">กำลังอ่านข้อมูลแมตช์…</div>;
  if (!digest) return <div className="a2k-empty">สรุปข้อมูลแมตช์ยังไม่พร้อม</div>;
  const view = buildMatchSummaryViewModel(digest);
  const { header, cards, scoreboard, draft } = view;
  const gameMode = typeof header.gameMode === "string"
    ? header.gameMode
    : typeof header.gameMode === "number" ? gameModeName(header.gameMode) : null;
  const gameModeReason = header.gameModeReason ?? (typeof header.gameMode === "number" && gameMode === null ? `ไม่รู้จักรหัสโหมด ${header.gameMode}` : undefined);
  const objectiveGroups = [
    { id: "wards", label: "วอร์ด", items: digest.objectives?.wards ?? [] },
    { id: "other_tracks", label: "อื่น ๆ", items: digest.objectives?.other_tracks ?? [] },
  ].filter((group) => group.items.length > 0);
  const unknownBuildingCount = (digest.objectives?.buildings ?? []).filter((item: any) => item.destroyed == null).length;
  const metricText = (metric: { value: number | null; reason: string | null }) => metric.value === null ? "— ไม่ทราบ" : Math.round(metric.value).toLocaleString("th-TH");
  const metricTitle = (metric: { value: number | null; reason: string | null }) => metric.value === null ? metric.reason ?? undefined : undefined;
  const sideNames: Record<string, string> = { radiant: "Radiant", dire: "Dire" };
  return <div className="a2k-summary">
    <header className="a2k-summary__header">
      <div><h2>สรุปแมตช์</h2><strong className={`a2k-summary__winner is-${header.winner ?? "unknown"}`}>{header.winnerText}</strong></div>
      <div className="a2k-summary__facts">
        <span>คะแนนสังหาร · Radiant <b title={metricTitle(header.kills.radiant)}>{metricText(header.kills.radiant)}</b> : <b title={metricTitle(header.kills.dire)}>{metricText(header.kills.dire)}</b> Dire</span>
        <span>เวลา {timeText(header.durationSeconds)}</span>
        <span title={gameModeReason}>โหมด {gameMode ?? "— ไม่ทราบ"}</span>
        <span title={header.patchReason ?? undefined}>แพตช์ {header.patch ?? "— ไม่ทราบ"}</span>
        {header.winnerSource && <span>ที่มาผลแมตช์ · {header.winnerSource}</span>}
      </div>
    </header>
    {header.warning && <p className="a2k-result-warning" role="alert">{header.warning}</p>}

    <OpponentsSection digest={digest} ownHeroId={ownHeroId} matchData={matchData} />

    <section className="a2k-summary__hero-teams" aria-label="การ์ดฮีโร่ทั้งสองทีม">
      <h3>ฮีโร่ในแมตช์</h3>
      {(["radiant", "dire"] as const).map((side) => <section className={`a2k-summary__hero-team is-${side}`} key={side} aria-label={`ฮีโร่ทีม ${sideNames[side]}`}>
        <h4>{sideNames[side]}</h4>
        <div className="a2k-summary__hero-scroll" role="region" aria-label={`การ์ดฮีโร่ ${sideNames[side]}`} tabIndex={0}>
          <div className="a2k-summary__hero-track">{cards[side].length ? cards[side].map((hero, index) => <article className="a2k-summary__hero-card" key={hero.heroId ?? `${side}-${index}`}>
            {hero.heroIconName ? <img src={localSummaryIcon("hero", hero.heroIconName)} alt="" aria-hidden="true" /> : <span className="a2k-summary__hero-icon-unknown" aria-hidden="true">?</span>}
            <strong>{hero.hero ?? "— ไม่ทราบ"}</strong>
            <span className="a2k-summary__hero-side">{sideNames[side]}</span>
            <span className="a2k-summary__lane" title={hero.laneReason ?? undefined}>{hero.lane ? LANE_NAMES[hero.lane] ?? "— ไม่ทราบ" : "— ไม่ทราบเลน"}</span>
            <span className={`a2k-summary__lane-result${hero.laneResult ? "" : " is-unknown"}`} title={hero.laneResultReason ?? undefined}>{hero.laneResult ?? "— ไม่ทราบผลเลน"}</span>
            <span className="a2k-summary__kda" aria-label="สังหาร ตาย ช่วยเหลือ">
              <b title={metricTitle(hero.kda.k)}>{metricText(hero.kda.k)}</b> /
              <b title={metricTitle(hero.kda.d)}>{metricText(hero.kda.d)}</b> /
              <b title={metricTitle(hero.kda.a)}>{metricText(hero.kda.a)}</b>
            </span>
            <span className="a2k-summary__economy"><span>{hero.economyLabel}</span><b title={metricTitle(hero.economy)}>{metricText(hero.economy)}</b></span>
          </article>) : <p className="a2k-summary__unknown">— ไม่ทราบ · ไม่มีข้อมูลฮีโร่ของทีมนี้ใน digest</p>}</div>
        </div>
      </section>)}
    </section>

    <section className="a2k-summary__middle" aria-label="ความได้เปรียบ แผนที่ และผลเลน">
      <AdvantageChart digest={digest} onSeek={onSeek} />
      <BuildingLosses digest={digest} mapCalibration={mapCalibration} onSeek={onSeek}
        mapImageUrl={replayMapUrlForPatch(matchData?.meta?.patch ?? matchData?.patch)} />
      <div className="a2k-summary__lane-results"><h3>ผลเลน</h3><LaneCards digest={digest} onSeek={onSeek} /></div>
    </section>

    <section className="a2k-scoreboards" aria-label="ตารางสถิติรายทีม">
      <h3>ตารางสถิติ</h3>
      {(["radiant", "dire"] as const).map((side) => {
        const team = scoreboard[side];
        return <section className={`a2k-scoreboard is-${side}`} key={side} aria-label={`ตาราง ${sideNames[side]}`}>
          <h4>{sideNames[side]}</h4>
          <div className="a2k-scoreboard__scroll" role="region" aria-label={`เลื่อนตาราง ${sideNames[side]} ในแนวนอน`} tabIndex={0}>
            <table><thead><tr><th>ฮีโร่</th><th>K</th><th>D</th><th>A</th><th>NW</th><th>LH / DN</th><th>GPM / XPM</th><th>Hero damage</th><th>ไอเท็มสุดท้าย · นาทีที่ซื้อ</th></tr></thead>
              <tbody>{team.rows.map((row, index) => <tr key={row.heroId ?? `${side}-${index}`}>
                <th scope="row"><span className="a2k-scoreboard__hero">{row.heroIconName ? <img src={localSummaryIcon("hero", row.heroIconName)} alt="" aria-hidden="true" /> : <span className="a2k-summary__hero-icon-unknown" aria-hidden="true">?</span>}{row.hero ?? "— ไม่ทราบ"}</span></th>
                <td title={metricTitle(row.metrics.k)}>{metricText(row.metrics.k)}</td>
                <td title={metricTitle(row.metrics.d)}>{metricText(row.metrics.d)}</td>
                <td title={metricTitle(row.metrics.a)}>{metricText(row.metrics.a)}</td>
                <td title={metricTitle(row.metrics.nw)}>{metricText(row.metrics.nw)}</td>
                <td title={`${metricTitle(row.metrics.lh) ?? ""} ${metricTitle(row.metrics.dn) ?? ""}`.trim() || undefined}>{metricText(row.metrics.lh)} / {metricText(row.metrics.dn)}</td>
                <td title={`${metricTitle(row.metrics.gpm) ?? ""} ${metricTitle(row.metrics.xpm) ?? ""}`.trim() || undefined}>{metricText(row.metrics.gpm)} / {metricText(row.metrics.xpm)}</td>
                <td title={metricTitle(row.metrics.heroDamage)}>{metricText(row.metrics.heroDamage)}</td>
                <td>{!row.itemsKnown ? <span title={row.itemsReason ?? undefined}>— ไม่ทราบ</span> : row.items.length === 0 ? "ไม่มีไอเท็มในช่องที่บันทึก" :
                  <ul className="a2k-scoreboard__items">{row.items.map((item, itemIndex) => <li key={`${item.id}-${itemIndex}`} title={item.id}>
                    <img src={localSummaryIcon("item", item.iconName ?? item.id)} alt={item.id} />
                    {item.purchaseMinute !== null && <span>{item.purchaseMinute}m</span>}
                  </li>)}</ul>}</td>
              </tr>)}
                <tr className="a2k-scoreboard__total"><th scope="row">รวมทีม · {team.totals.playerCount} ฮีโร่</th>
                  <td title={metricTitle(team.totals.metrics.k)}>{metricText(team.totals.metrics.k)}</td>
                  <td title={metricTitle(team.totals.metrics.d)}>{metricText(team.totals.metrics.d)}</td>
                  <td title={metricTitle(team.totals.metrics.a)}>{metricText(team.totals.metrics.a)}</td>
                  <td title={metricTitle(team.totals.metrics.nw)}>{metricText(team.totals.metrics.nw)}</td>
                  <td title={`${metricTitle(team.totals.metrics.lh) ?? ""} ${metricTitle(team.totals.metrics.dn) ?? ""}`.trim() || undefined}>{metricText(team.totals.metrics.lh)} / {metricText(team.totals.metrics.dn)}</td>
                  <td title={`${metricTitle(team.totals.metrics.gpm) ?? ""} ${metricTitle(team.totals.metrics.xpm) ?? ""}`.trim() || undefined}>{metricText(team.totals.metrics.gpm)} / {metricText(team.totals.metrics.xpm)}</td>
                  <td title={metricTitle(team.totals.metrics.heroDamage)}>{metricText(team.totals.metrics.heroDamage)}</td><td>—</td>
                </tr>
              </tbody>
            </table>
          </div>
        </section>;
      })}
    </section>

    <section className="a2k-draft" aria-label="ลำดับการเลือกและแบนฮีโร่">
      <h3>ดราฟต์ · เลือกและแบนตามลำดับ</h3>
      {draft.state === "unknown" ? <p title={draft.reason ?? undefined}>— ไม่ทราบ · {draft.reason ?? "ไม่มีข้อมูลดราฟต์"}</p>
        : draft.state === "empty" ? <p>ไม่มีรายการเลือกหรือแบนในข้อมูลรีเพลย์</p>
          : <div className="a2k-draft__teams">{(["radiant", "dire", "unknown"] as const).map((side) => {
            const entries = draft.groups[side];
            if (side !== "unknown" && entries.length === 0) return null;
            if (side === "unknown" && entries.length === 0) return null;
            return <section key={side} className={`a2k-draft__team is-${side}`}>
              <h4>{side === "unknown" ? "ทีมไม่ทราบ" : sideNames[side]}</h4>
              <ol>{entries.map((entry) => <li key={`${entry.order}-${entry.kind}`}><span className="a2k-draft__order">{entry.order}</span>
                <span>{entry.kind === "Pick" ? "เลือก" : entry.kind === "Ban" ? "แบน" : "— ไม่ทราบ"}</span>
                {entry.heroIconName ? <img src={localSummaryIcon("hero", entry.heroIconName)} alt="" aria-hidden="true" /> : <span className="a2k-summary__hero-icon-unknown" aria-hidden="true">?</span>}
                <span>{entry.hero}</span>
              </li>)}</ol>
            </section>;
          })}</div>}
    </section>

    <TeamfightSummary digest={digest} onSeek={onSeek} />

    <section className="a2k-objectives"><h3>วัตถุประสงค์</h3>
      {unknownBuildingCount > 0 && <small>ไม่ทราบสถานะอาคาร {unknownBuildingCount} รายการ</small>}
      {objectiveGroups.length === 0 ? <p>{Array.isArray(digest.objectives?.wards) && Array.isArray(digest.objectives?.other_tracks)
        ? "ไม่พบวอร์ดหรือวัตถุประสงค์อื่นใน digest"
        : `— ไม่ทราบ · ${digest.objectives?.wards_reason ?? digest.objectives?.other_tracks_reason ?? "objective tracks are missing"}`}</p> : objectiveGroups.map((group) => <details key={group.id}>
        <summary>{group.label} ({group.items.length})</summary><ul className="a2k-event-rows">{group.items.map((item: any, index: number) => {
          const time = item.destroyed_at_s ?? item.start_s ?? item.t;
          return <li key={`${group.id}-${index}`}><span aria-hidden="true">{objectiveIcon(group.id)}</span>
            {typeof time === "number" ? <button type="button" title={item.name ?? undefined} onClick={() => onSeek?.(time)}>{timeText(time)}</button> : <span>— ไม่ทราบ</span>}
            <span>{typeof item.name === "string" ? item.name : group.label}</span></li>;
        })}</ul>
      </details>)}
    </section>
  </div>;
}

/** Compact post-game arrangement of the same UI-023 digest model and summary blocks. */
export function PostGameMatchSummaryBlocks({ digest, onSeek, ownHeroId, compact = false }: Pick<KnowledgePanelProps, "digest" | "onSeek" | "ownHeroId"> & { compact?: boolean }) {
  if (!digest) return <div className="a2k-empty">สรุปข้อมูลแมตช์ยังไม่พร้อม</div>;
  const view = buildMatchSummaryViewModel(digest);
  const score = (metric: { value: number | null }) => metric.value === null ? "— ไม่ทราบ" : Math.round(metric.value).toLocaleString("th-TH");
  const heroName = (row: { heroId: string | null; hero: string | null }) => {
    const key = row.heroId ?? row.hero;
    if (!key) return "— ไม่ทราบ";
    const heroId = key.startsWith("npc_dota_hero_") ? key : `npc_dota_hero_${key}`;
    return displayHeroName(heroId)?.name ?? key;
  };
  const compactLimit = compact ? 1 : 2;
  const rows = (["radiant", "dire"] as const).flatMap((side) => view.scoreboard[side].rows.slice(0, 5).map((row, index) => ({ side, row, key: row.heroId ?? `${side}-${index}` })));
  return <div className="a2k-postgame-layout" data-testid="post-game-stage-two-layout">
    <OpponentsSection digest={digest} ownHeroId={ownHeroId} />
    <div className="a2k-postgame-main-row">
      <div className="a2k-postgame-chart"><h3 className="a2k-postgame-chart-title">ความได้เปรียบ</h3><AdvantageChart digest={digest} onSeek={onSeek} /></div>
      <section className="a2k-postgame-scoreboard" aria-label="ตารางฮีโร่ทั้งสิบพร้อมไอเท็ม">
        <h3>ตารางฮีโร่ · 10 คน</h3>
        <table className="a2k-postgame-score-table"><thead><tr><th>ฮีโร่</th><th>K/D/A</th><th>NW</th><th>ไอเท็ม</th></tr></thead>
          <tbody>{rows.map(({ side, row, key }) => <tr key={key}>
            <th scope="row"><span className={`a2k-postgame-score-side is-${side}`} title={SIDE_NAMES[side]}>{side === "radiant" ? "R" : "D"}</span>
              {row.heroIconName ? <img className="a2k-postgame-score-hero-icon" src={localSummaryIcon("hero", row.heroIconName)} alt="" aria-hidden="true" /> : null}
              <span className="a2k-postgame-score-hero" title={heroName(row)}>{heroName(row)}</span></th>
            <td title={[row.metrics.k.reason, row.metrics.d.reason, row.metrics.a.reason].filter(Boolean).join(" · ") || undefined}>{score(row.metrics.k)} / {score(row.metrics.d)} / {score(row.metrics.a)}</td>
            <td title={row.metrics.nw.reason ?? undefined}>{score(row.metrics.nw)}</td>
            <td>{!row.itemsKnown ? <span title={row.itemsReason ?? undefined}>— ไม่ทราบ</span> : row.items.length === 0 ? "ไม่มีไอเท็ม" : <span className="a2k-postgame-score-items">
              {row.items.slice(0, 2).map((item, index) => <img key={`${item.id}-${index}`} src={localSummaryIcon("item", item.iconName ?? item.id)} alt="" title={item.id} aria-hidden="true" />)}
              {row.items.length > 2 && <span>+{row.items.length - 2}</span>}
            </span>}</td>
          </tr>)}</tbody>
        </table>
        {rows.length < 10 && <small className="a2k-postgame-more">— ไม่ทราบ · digest มีข้อมูล {rows.length} จาก 10 ฮีโร่</small>}
      </section>
    </div>
    <div className="a2k-postgame-details-row">
      <BuildingLosses digest={digest} mapCalibration={null} onSeek={onSeek} compact compactLimit={compactLimit} />
      <section className="a2k-postgame-lanes"><h3>ผลเลน</h3><LaneCards digest={digest} onSeek={onSeek} compact compactLimit={compactLimit} /></section>
      <TeamfightSummary digest={digest} onSeek={onSeek} compact compactLimit={compactLimit} />
    </div>
  </div>;
}


/** Local hero comparison tab; selection IDs are always roster hero IDs. */
export function HeroComparisonTab({ digest, selectedHeroId, opponentHeroId, onSelectHero, onSelectOpponentHero }: Pick<KnowledgePanelProps, "digest" | "selectedHeroId" | "opponentHeroId" | "onSelectHero" | "onSelectOpponentHero">) {
  const heroes = digest?.heroes ?? [];
  const selected = heroes.find((hero) => hero.id === selectedHeroId) ?? heroes[0];
  const lane = selected?.lane?.lane ?? null;
  const opponent = heroes.find((hero) => hero.id === opponentHeroId) ?? heroes.find((hero) => hero.side !== selected?.side && hero.lane?.lane === lane) ?? heroes.find((hero) => hero.side !== selected?.side);
  const rows = matchupRows(heroes.map((hero) => ({ ...hero, lane: profileLane(hero) })));
  const comparisonRows = [
    ["ครีปที่สังหาร ณ 10:00", "at_10.lh"], ["ครีปที่ปฏิเสธ ณ 10:00", "at_10.dn"],
    ["มูลค่าสุทธิ ณ 10:00", "at_10.nw"], ["ประสบการณ์ ณ 10:00", "at_10.xp"],
    ["มูลค่าสุทธิ/นาที", "rates.nw_per_min"], ["ประสบการณ์/นาที", "rates.xpm"],
    ["ความเสียหายต่อฮีโร่", "hero_damage"], ["ทองที่หาได้/นาที", "rates.gpm"],
    ["สังหาร / เสียชีวิต", "final"], ["เวลาที่เสียชีวิต (วินาที)", "dead_s"],
    ["รูนที่เก็บ", "runes"], ["ทีมไฟต์ที่เข้าร่วม", "teamfights_present"],
  ] as const;
  const readMetric = (hero: KnowledgeHero | undefined, path: string) => {
    if (path === "hero_damage") return heroComparisonEngineMetric(hero, "hd");
    if (path === "rates.gpm") return heroComparisonEngineMetric(hero, "gpm");
    if (!hero) return "— ไม่ทราบ";
    if (path === "final") return `${valueAt((hero.final as any)?.k)} / ${valueAt((hero.final as any)?.d)}`;
    const value = path.split(".").reduce<any>((current, key) => current?.[key], hero as any);
    return Array.isArray(value) ? `${value.length}` : valueAt(value);
  };
  return <div className="a2k-comparison"><h2>เทียบเลน/ฮีโร่</h2><MatchupStrip digest={digest} selectedHeroId={selected?.id} onSelectHero={onSelectHero} />
    {selected && <section>
      <label>ฮีโร่ที่เลือก <select value={selected.id} onChange={(event) => onSelectHero?.(event.target.value)}>{heroes.map((hero) => <option key={hero.id} value={hero.id}>{hero.hero} · {SIDE_NAMES[hero.side]}</option>)}</select></label>
      {opponent && <label>ฮีโร่ฝั่งตรงข้าม <select value={opponent.id} onChange={(event) => onSelectOpponentHero?.(event.target.value)}>{heroes.filter((hero) => hero.side !== selected.side).map((hero) => <option key={hero.id} value={hero.id}>{hero.hero} · {LANE_NAMES[hero.lane?.lane ?? "unknown"]}</option>)}</select></label>}
      <HeroBenchmarksPanel hero={selected} durationSeconds={typeof digest?.meta?.duration_s === "number" ? digest.meta.duration_s : null} />
      <table><thead><tr><th>ข้อมูลเปรียบเทียบ</th><th>{selected.hero}</th><th>{opponent?.hero ?? "คู่เทียบไม่ทราบ"}</th></tr></thead><tbody>{comparisonRows.map(([label, path]) => <tr key={label}><th>{label}</th><td>{readMetric(selected, path)}</td><td>{readMetric(opponent, path)}</td></tr>)}
      </tbody></table>
      <div className="a2k-phase-table"><h3>เปรียบเทียบตามช่วง</h3>{["laning", "mid", "late"].map((phase) => <p key={phase}><b>{phaseLabel(phase)}</b> · {selected.hero}: สังหาร {valueAt((selected.phases as any)?.[phase]?.k)} / เสียชีวิต {valueAt((selected.phases as any)?.[phase]?.d)} · {opponent?.hero ?? "คู่เทียบไม่ทราบ"}: สังหาร {valueAt((opponent?.phases as any)?.[phase]?.k)} / เสียชีวิต {valueAt((opponent?.phases as any)?.[phase]?.d)}</p>)}</div>
      <section className="a2k-comparison-items"><h3>ไอเท็ม</h3><div><strong>{selected.hero}</strong><ItemRows value={selected.items ?? selected.final_items} /></div><div><strong>{opponent?.hero ?? "คู่เทียบไม่ทราบ"}</strong><ItemRows value={opponent?.items ?? opponent?.final_items} /></div></section>
    </section>}
    <section><h3>การจับคู่ตามเลน</h3>{rows.filter((row) => row.heroes.length > 0).map((row) => <p key={row.lane}><b>{LANE_NAMES[row.lane]}</b> · {row.heroes.map((hero) => hero.hero).join(" ↔ ")}</p>)}</section>
  </div>;
}

/** Persisted AI report view. Generation can only start from the explicit button. */
export function AiReportTab({ reports = [], selectedReportId, onSelectReport, onGenerateReport, onRegenerateReport, costEstimateText, renderMarkdown, generatingReport = false, reportDisabled = false }: Pick<KnowledgePanelProps, "reports" | "selectedReportId" | "onSelectReport" | "onGenerateReport" | "onRegenerateReport" | "costEstimateText" | "renderMarkdown" | "generatingReport" | "reportDisabled">) {
  const report = reports.find((item) => item.id === selectedReportId) ?? reports[0];
  const stale = report?.stale_reasons?.length ? report.stale_reasons : report?.meta?.stale ? ["ผลวิเคราะห์ไม่ตรงกับข้อมูลปัจจุบัน"] : [];
  return <div className="a2k-report"><h2>บทวิเคราะห์ AI</h2>
    <p>เมื่อกด ระบบจะส่งสรุปแมตช์ให้ผู้ช่วยที่เลือกไว้ หากผู้ให้บริการรองรับการอ่านข้อมูลเพิ่มเติม ระบบจะอ่านข้อมูลจากเครื่องเมื่อจำเป็น โดยไม่ส่งประวัติแชต</p>
    <p className="a2k-report__cost">{costEstimateText ?? "ประมาณค่าใช้จ่ายไม่ได้ในขณะนี้ · ต้องตรวจสอบราคาผู้ให้บริการก่อนส่งคำขอ"}</p>
    <button type="button" className="a2k-primary" disabled={reportDisabled || generatingReport} onClick={report ? onRegenerateReport ?? onGenerateReport : onGenerateReport}>{generatingReport ? "กำลังสร้าง…" : report ? "สร้างใหม่" : "สร้างบทวิเคราะห์"}</button>
    {reports.length > 0 && <label>รายงานที่บันทึกไว้ <select value={report?.id ?? ""} onChange={(event) => onSelectReport?.(event.target.value)}>{reports.map((item, index) => <option key={item.id} value={item.id}>รายงานที่ {index + 1} · {new Date(item.meta?.created_unix_ms ?? 0).toLocaleString("th-TH")}</option>)}</select></label>}
    {report && <><header><span>สร้างเมื่อ {new Date(report.meta?.created_unix_ms ?? 0).toLocaleString("th-TH")}</span>{stale.length > 0 && <strong className="a2k-stale">ข้อมูลเปลี่ยนแล้ว</strong>}</header><article>{renderMarkdown ? renderMarkdown(report.markdown ?? "") : <pre>{report.markdown ?? "กำลังโหลดรายงาน…"}</pre>}</article></>}
    {!report && <div className="a2k-empty">ยังไม่มีรายงาน บทวิเคราะห์จะสร้างเมื่อกดปุ่มด้านบนเท่านั้น</div>}
  </div>;
}

/** Three host-mounted left-panel tabs; the Notes tab remains owned by the host page. */
export function KnowledgeTabs({ activeTab, onSelectTab }: { activeTab: "summary" | "compare" | "ai" | "notes"; onSelectTab: (tab: "summary" | "compare" | "ai" | "notes") => void }) {
  return <nav className="a2k-tabs" role="tablist">{ANALYTICS_TABS.map(([id, title]) => <button key={id} type="button" role="tab" aria-selected={activeTab === id} className={activeTab === id ? "is-active" : ""} onClick={() => onSelectTab(id)}>{title}</button>)}</nav>;
}
