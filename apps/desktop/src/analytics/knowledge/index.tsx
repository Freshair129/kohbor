import type { ReactNode } from "react";
import { buildLineSegments, heroComparisonEngineMetric, heroMetricValue, laneBadgeLabel, laneExpectationReading, matchResultPresentation, matchupRows, orderedDraftEntries, showLaneExpectationInStrip } from "./presentationModel.mjs";
import "./knowledge.css";

export type KnowledgeHero = {
  id: string;
  hero: string;
  side: "radiant" | "dire";
  slot: number;
  lane?: { lane: "top" | "mid" | "bot" | null; share?: number | null; reason?: string };
  final?: Record<string, { value?: number | null; reason?: string } | number | null>;
  rates?: { gpm?: number | null; gpm_reason?: string | null; gpm_source?: string | null };
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
  provisional_thresholds?: { label?: string };
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

/** SVG chart for the local digest. Unknown samples split the line into separate segments. */
export function AdvantageChart({ digest, onSeek }: Pick<KnowledgePanelProps, "digest" | "onSeek">) {
  const rows = digest?.advantage_per_min ?? [];
  const width = 560;
  const height = 190;
  const pad = 18;
  const series = [
    { key: "radiant_nw_diff", label: "ส่วนต่างมูลค่าสุทธิ", color: "#7aa2f7" },
    { key: "radiant_xp_diff", label: "ส่วนต่าง XP", color: "#e6a85c" },
  ];
  const points = series.map(({ key }) => rows.map((row) => typeof row[key] === "number" ? row[key] as number : null));
  const finite = points.flat().filter((value): value is number => value !== null && Number.isFinite(value));
  const domain: [number, number] = finite.length ? [Math.min(...finite), Math.max(...finite)] : [0, 1];
  const zeroY = domain[0] <= 0 && domain[1] >= 0 && domain[0] !== domain[1]
    ? pad + ((domain[1] - 0) * (height - pad * 2)) / (domain[1] - domain[0])
    : height / 2;
  const duration = typeof digest?.meta?.duration_s === "number" ? digest.meta.duration_s : 0;
  return <section className="a2k-chart" aria-label="กราฟความได้เปรียบตามเวลา">
    <div className="a2k-chart__legend">{series.map((item) => <span key={item.key}><i style={{ background: item.color }} />{item.label}</span>)}</div>
    <svg viewBox={`0 0 ${width} ${height}`} role="img" aria-label="กราฟส่วนต่าง Radiant ต่อ Dire">
      <line x1={pad} x2={width - pad} y1={zeroY} y2={zeroY} className="a2k-chart__zero" />
      {series.map((item, seriesIndex) => buildLineSegments(points[seriesIndex] ?? [], width - pad * 2, height - pad * 2, domain).map((segment, index) => {
        const coords = segment.map(({ x, y }) => `${x + pad},${y + pad}`).join(" ");
        return <polyline key={`${item.key}-${index}`} points={coords} fill="none" stroke={item.color} strokeWidth="3" strokeLinejoin="round" strokeLinecap="round" />;
      }))}
      {(digest?.deaths ?? []).filter((death) => death.killer != null && typeof death.time_s === "number").map((death) => <circle key={String(death.event_ref)} cx={pad + (duration > 0 ? (death.time_s! / duration) * (width - pad * 2) : 0)} cy={zeroY} r="3" fill="#dc6e76" opacity="0.8" />)}
      {rows.map((row) => typeof row.minute === "number" && <rect key={`seek-${row.minute}`} x={pad + (row.minute * (width - pad * 2)) / Math.max(rows.length - 1, 1) - 4} y={pad} width="8" height={height - pad * 2} fill="transparent" role="button" tabIndex={0} aria-label={`ไปนาที ${row.minute}`} onClick={() => onSeek?.(row.minute * 60)} onKeyDown={(event) => { if (event.key === "Enter" || event.key === " ") onSeek?.(row.minute * 60); }} />)}
    </svg>
    <div className="a2k-chart__axis"><span>0:00</span><span>{timeText(digest?.meta?.duration_s)}</span></div>
    <small>ช่องว่างหมายถึงไม่มี sample ในข้อมูลต้นทาง · แสดงตัวเลขตามเวลาที่มีจริง</small>
  </section>;
}

/** The three lane cards display the digest outcome and its provisional basis. */
export function LaneCards({ digest, onSeek }: Pick<KnowledgePanelProps, "digest" | "onSeek">) {
  const lanes = digest?.lanes ?? [];
  return <div className="a2k-lanes">{["top", "mid", "bot"].map((lane) => {
    const row = lanes.find((item) => item.lane === lane);
    const radiantReading = laneExpectationReading(row, "radiant");
    const direReading = laneExpectationReading(row, "dire");
    const score = typeof row?.score?.value === "number" ? `${row.score.value >= 0 ? "+" : ""}${row.score.value.toFixed(1)}` : "— ไม่ทราบ";
    const expected = typeof row?.expectation?.value === "number" ? `${row.expectation.value >= 0 ? "+" : ""}${row.expectation.value.toFixed(1)}` : null;
    return <article key={lane} className="a2k-lane-card">
      <header><strong>{LANE_NAMES[lane]}</strong><span>{laneBadgeLabel(row?.outcome ?? null)}</span></header>
      <div className="a2k-lane-card__teams">{(["radiant", "dire"] as const).map((side) => <p key={side}><b>{SIDE_NAMES[side]}</b> {(row?.[side] ?? []).map((hero: any) => hero.name ?? hero.hero).join(" · ") || "— ไม่ทราบ"}</p>)}</div>
      <dl><dt>NW ต่างกันที่ 10:00</dt><dd>{valueAt(row?.nw_diff_10)}</dd><dt>XP ต่างกันที่ 10:00</dt><dd>{valueAt(row?.xp_diff_10)}</dd></dl>
      {row?.outcome_reason && <small>{row.outcome_reason}</small>}
      <section className="a2k-lane-reading" aria-label="คะแนนเลนเทียบค่าคาดหมาย">
        <header><strong>คะแนนเลน (เกณฑ์ของเกาะเบาะ)</strong><span>{row?.label ?? "ทดลอง · เกณฑ์ชั่วคราว"}</span></header>
        <dl><dt>คะแนนจริง · มุม Radiant</dt><dd>{score}</dd>
          <dt>ค่าคาดหมาย</dt><dd>{expected ?? "ยังไม่มีค่าคาดหมาย"}</dd>
          {expected !== null && <><dt>แหล่งค่าคาดหมาย</dt><dd>{row?.expectation?.source_label ?? "— ไม่ทราบ"}</dd></>}
          <dt>องค์ประกอบที่ใช้</dt><dd>{(row?.score?.components_used ?? []).join(" · ") || "— ไม่ทราบ"}</dd>
          {row?.personal_average && <><dt>ค่าเฉลี่ยของคุณ</dt><dd>{row.personal_average.value >= 0 ? "+" : ""}{row.personal_average.value.toFixed(1)} · n={row.personal_average.n}</dd></>}
        </dl>
        {expected === null ? <p className="a2k-lane-reading__unknown">ยังไม่มีค่าคาดหมาย · {row?.expectation_reason ?? "— ไม่ทราบ"}</p> : <>
          <p className="a2k-lane-reading__verdict">Radiant · {radiantReading.verdict ?? "— ไม่ทราบ"} · {radiantReading.trialLabel}</p>
          <p className="a2k-lane-reading__verdict">Dire · {direReading.verdict ?? "— ไม่ทราบ"} · {direReading.trialLabel}</p>
          <small>{radiantReading.explanation}</small>
        </>}
      </section>
      {typeof row?.boundary_s === "number" ? <button type="button" onClick={() => onSeek?.(row.boundary_s)}>ไปที่ {timeText(row.boundary_s)}</button> : <small>เวลาแบ่งเลน: — ไม่ทราบ</small>}
    </article>;
  })}</div>;
}

/** Compact matchup strip for lane-by-lane hero assignment; unknown positions remain visible. */
export function MatchupStrip({ digest, selectedHeroId, onSelectHero }: Pick<KnowledgePanelProps, "digest" | "selectedHeroId" | "onSelectHero">) {
  const heroes = digest?.heroes ?? [];
  const rows = matchupRows(heroes.map((hero) => ({ ...hero, lane: profileLane(hero) })));
  return <div className="a2k-matchups">{rows.map((row) => <section key={row.lane} className="a2k-matchups__lane">
    <strong>{LANE_NAMES[row.lane]}</strong>
    {row.heroes.length === 0 ? <span className="a2k-muted">— ไม่ทราบ</span> : row.heroes.map((hero) => <button key={hero.id} type="button" className={selectedHeroId === hero.id ? "is-selected" : ""} onClick={() => onSelectHero?.(hero.id)}>
      <i className={`a2k-side-dot is-${hero.side}`} />{hero.hero}<small>{SIDE_NAMES[hero.side]}</small>
      {showLaneExpectationInStrip(row.heroes, digest?.lanes?.find((item) => item.lane === row.lane)?.constants?.lane_strip_max_cells) && (() => {
        const laneDigest = digest?.lanes?.find((item) => item.lane === row.lane);
        const reading = laneExpectationReading(laneDigest, hero.side);
        return reading.verdict ? <small className="a2k-matchups__expectation" title={reading.explanation ?? undefined}>{reading.verdict} · {reading.trialLabel}</small> : null;
      })()}
    </button>)}
  </section>)}</div>;
}

/** Summary tab for the free, locally computed K0-K2 match digest. */
export function MatchSummaryTab({ digest, loading, onSeek }: Pick<KnowledgePanelProps, "digest" | "loading" | "onSeek">) {
  if (loading) return <div className="a2k-empty" role="status">กำลังอ่านข้อมูลแมตช์…</div>;
  if (!digest) return <div className="a2k-empty">ยังไม่มี digest สำหรับแมตช์นี้</div>;
  const objectiveRows = [
    ...(digest.objectives?.buildings ?? []).filter((item: any) => item.destroyed === true),
    ...(digest.objectives?.wards ?? []),
    ...(digest.objectives?.other_tracks ?? []),
  ];
  const unknownBuildingCount = (digest.objectives?.buildings ?? []).filter((item: any) => item.destroyed == null).length;
  const result = matchResultPresentation(digest.meta?.result);
  const draft = orderedDraftEntries(digest.meta?.draft);
  return <div className="a2k-summary">
    <header className="a2k-summary__header"><h2>สรุปแมตช์</h2><span>{valueAt(digest.meta?.patch)} · {timeText(digest.meta?.duration_s)}</span></header>
    <p className="a2k-summary__score">Radiant {valueAt(digest.meta?.kills?.radiant)} — {valueAt(digest.meta?.kills?.dire)} Dire · ผลแมตช์ {result.text}{result.source ? ` · ${result.source}` : ""}</p>
    {result.warning && <p className="a2k-result-warning" role="alert">{result.warning}</p>}
    <section className="a2k-draft" aria-label="ลำดับการเลือกและแบนฮีโร่">
      <h3>ลำดับ Draft</h3>
      {draft === null ? <p title={digest.meta?.draft_reason ?? undefined}>—</p> : draft.length === 0 ? <p>ไม่มีข้อมูล pick/ban ใน replay</p> :
        <ol>{draft.map((entry) => <li key={entry.order}><span>{entry.order}</span> · {entry.team} · {entry.kind} · {entry.hero}</li>)}</ol>}
    </section>
    <section className="a2k-engine-hero-stats" aria-label="สถิติฮีโร่จาก replay engine">
      <h3>สถิติฮีโร่จากรีเพลย์</h3>
      <div className="a2k-engine-hero-stats__head"><b>ฮีโร่</b><b>HD</b><b>GPM</b></div>
      {(digest.heroes ?? []).map((hero) => <div className="a2k-engine-hero-stats__row" key={hero.id}>
        <span>{hero.hero}</span>
        <span title={hero.hero_damage_reason ?? (hero.hero_damage_source ? `ที่มา: ${hero.hero_damage_source}` : undefined)}>{heroMetricValue(hero.hero_damage)}</span>
        <span title={hero.rates?.gpm_reason ?? (hero.rates?.gpm_source ? `ที่มา: ${hero.rates.gpm_source}` : undefined)}>{heroMetricValue(hero.rates?.gpm)}</span>
      </div>)}
    </section>
    <AdvantageChart digest={digest} onSeek={onSeek} />
    <LaneCards digest={digest} onSeek={onSeek} />
    <section><h3>จังหวะสำคัญ</h3><ul>{(digest.teamfights ?? []).map((fight) => <li key={fight.n}><button onClick={() => onSeek?.(Number(fight.t?.[0] ?? 0))}>{timeText(fight.t?.[0])}–{timeText(fight.t?.[1])}</button> · ทีมไฟต์ {valueAt(fight.n)} · ตาย {fight.deaths?.length ?? "— ไม่ทราบ"}</li>)}</ul></section>
    <section><h3>วัตถุประสงค์</h3>{unknownBuildingCount > 0 && <small>ไม่ทราบสถานะอาคาร {unknownBuildingCount} รายการ</small>}<ul>{objectiveRows.map((item: any, index) => { const time = item.destroyed_at_s ?? item.start_s ?? item.t; return <li key={`${item.kind}-${index}`}><button onClick={() => typeof time === "number" && onSeek?.(time)}>{timeText(time)}</button> · {item.kind} · {item.name ?? "— ไม่ทราบ"}</li>; })}</ul></section>
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
    ["LH ที่ 10:00", "at_10.lh"], ["Deny ที่ 10:00", "at_10.dn"],
    ["มูลค่าสุทธิที่ 10:00", "at_10.nw"], ["XP ที่ 10:00", "at_10.xp"],
    ["NW/นาที", "rates.nw_per_min"], ["XPM", "rates.xpm"],
    ["ฮีโร่ดาเมจ (HD)", "hero_damage"], ["ทองที่หาได้/นาที (GPM)", "rates.gpm"],
    ["K / D", "final"], ["เวลาตาย (วินาที)", "dead_s"],
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
  const engineMetricTitle = (hero: KnowledgeHero | undefined, path: string) => {
    if (path === "hero_damage") return hero?.hero_damage_reason ?? (hero?.hero_damage_source ? `ที่มา: ${hero.hero_damage_source}` : undefined);
    if (path === "rates.gpm") return hero?.rates?.gpm_reason ?? (hero?.rates?.gpm_source ? `ที่มา: ${hero.rates.gpm_source}` : undefined);
    return undefined;
  };
  return <div className="a2k-comparison"><h2>เทียบเลน/ฮีโร่</h2><MatchupStrip digest={digest} selectedHeroId={selected?.id} onSelectHero={onSelectHero} />
    {selected && <section>
      <label>ฮีโร่ที่เลือก <select value={selected.id} onChange={(event) => onSelectHero?.(event.target.value)}>{heroes.map((hero) => <option key={hero.id} value={hero.id}>{hero.hero} · {SIDE_NAMES[hero.side]}</option>)}</select></label>
      {opponent && <label>ฮีโร่ฝั่งตรงข้าม <select value={opponent.id} onChange={(event) => onSelectOpponentHero?.(event.target.value)}>{heroes.filter((hero) => hero.side !== selected.side).map((hero) => <option key={hero.id} value={hero.id}>{hero.hero} · {LANE_NAMES[hero.lane?.lane ?? "unknown"]}</option>)}</select></label>}
      <table><thead><tr><th>ตัวเลข</th><th>{selected.hero}</th><th>{opponent?.hero ?? "คู่เทียบไม่ทราบ"}</th></tr></thead><tbody>{comparisonRows.map(([label, path]) => <tr key={label}><th>{label}</th><td title={engineMetricTitle(selected, path)}>{readMetric(selected, path)}</td><td title={engineMetricTitle(opponent, path)}>{readMetric(opponent, path)}</td></tr>)}
      </tbody></table>
      <div className="a2k-phase-table"><h3>เปรียบเทียบตามช่วง</h3>{["laning", "mid", "late"].map((phase) => <p key={phase}><b>{phase}</b> · {selected.hero}: K {valueAt((selected.phases as any)?.[phase]?.k)} / D {valueAt((selected.phases as any)?.[phase]?.d)} · {opponent?.hero ?? "คู่เทียบไม่ทราบ"}: K {valueAt((opponent?.phases as any)?.[phase]?.k)} / D {valueAt((opponent?.phases as any)?.[phase]?.d)}</p>)}</div>
      <p><b>ไอเท็ม</b> · {JSON.stringify(selected.items ?? "— ไม่ทราบ")} ↔ {JSON.stringify(opponent?.items ?? "— ไม่ทราบ")}</p>
    </section>}
    <section><h3>การจับคู่ตามเลน</h3>{rows.filter((row) => row.heroes.length > 0).map((row) => <p key={row.lane}><b>{LANE_NAMES[row.lane]}</b> · {row.heroes.map((hero) => hero.hero).join(" ↔ ")}</p>)}</section>
  </div>;
}

/** Persisted AI report view. Generation can only start from the explicit button. */
export function AiReportTab({ reports = [], selectedReportId, onSelectReport, onGenerateReport, onRegenerateReport, costEstimateText, renderMarkdown, generatingReport = false, reportDisabled = false }: Pick<KnowledgePanelProps, "reports" | "selectedReportId" | "onSelectReport" | "onGenerateReport" | "onRegenerateReport" | "costEstimateText" | "renderMarkdown" | "generatingReport" | "reportDisabled">) {
  const report = reports.find((item) => item.id === selectedReportId) ?? reports[0];
  const stale = report?.stale_reasons?.length ? report.stale_reasons : report?.meta?.stale ? ["ผลวิเคราะห์ไม่ตรงกับข้อมูลปัจจุบัน"] : [];
  const inputTokens = report?.meta?.usage?.input;
  const outputTokens = report?.meta?.usage?.output;
  const previousUsage = typeof inputTokens === "number" && typeof outputTokens === "number" ? `ครั้งก่อนใช้ ${inputTokens + outputTokens} tokens · ` : "";
  return <div className="a2k-report"><h2>บทวิเคราะห์ AI</h2>
    <p>เมื่อกด ระบบจะส่ง digest ของแมตช์ให้โมเดลที่เลือกใน Settings; หากผู้ให้บริการรองรับ tools จะเพิ่มข้อมูลผู้ชม ดัชนีโน้ต และบทวิเคราะห์ที่บันทึกไว้ แล้วอ่านข้อมูลใน bundle เฉพาะเมื่อจำเป็น โดยไม่ส่งประวัติแชต</p>
    <p className="a2k-report__cost">{previousUsage}{costEstimateText ?? "ประมาณค่าใช้จ่ายไม่ได้ในขณะนี้ · ต้องตรวจสอบราคาผู้ให้บริการก่อนส่งคำขอ"}</p>
    <button type="button" className="a2k-primary" disabled={reportDisabled || generatingReport} onClick={report ? onRegenerateReport ?? onGenerateReport : onGenerateReport}>{generatingReport ? "กำลังสร้าง…" : report ? "สร้างใหม่" : "สร้างบทวิเคราะห์"}</button>
    {reports.length > 0 && <label>รายงานที่บันทึกไว้ <select value={report?.id ?? ""} onChange={(event) => onSelectReport?.(event.target.value)}>{reports.map((item) => <option key={item.id} value={item.id}>{item.meta?.model ?? "model"} · {new Date(item.meta?.created_unix_ms ?? 0).toLocaleString("th-TH")}</option>)}</select></label>}
    {report && <><header><span>{report.meta?.model} · prompt v{report.meta?.prompt_version} · {report.meta?.usage?.output ?? "—"} tokens</span>{stale.length > 0 && <strong className="a2k-stale" title={stale.join(", ")}>ล้าสมัย</strong>}</header><article>{renderMarkdown ? renderMarkdown(report.markdown ?? "") : <pre>{report.markdown ?? "กำลังโหลดรายงาน…"}</pre>}</article></>}
    {!report && <div className="a2k-empty">ยังไม่มีรายงาน บทวิเคราะห์จะสร้างเมื่อกดปุ่มด้านบนเท่านั้น</div>}
  </div>;
}

/** Three host-mounted left-panel tabs; the Notes tab remains owned by the host page. */
export function KnowledgeTabs({ activeTab, onSelectTab }: { activeTab: "summary" | "compare" | "ai" | "notes" | "coach"; onSelectTab: (tab: "summary" | "compare" | "ai" | "notes" | "coach") => void }) {
  const tabs = [["summary", "สรุปแมตช์"], ["compare", "เทียบเลน/ฮีโร่"], ["coach", "ทบทวนโค้ช"], ["ai", "บทวิเคราะห์ AI"], ["notes", "โน้ต"]] as const;
  return <nav className="a2k-tabs" role="tablist">{tabs.map(([id, title]) => <button key={id} type="button" role="tab" aria-selected={activeTab === id} className={activeTab === id ? "is-active" : ""} onClick={() => onSelectTab(id)}>{title}</button>)}</nav>;
}
