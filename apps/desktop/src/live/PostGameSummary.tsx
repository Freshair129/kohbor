import { useLayoutEffect, useRef, useState } from "react";
import { PostGameMatchSummaryBlocks } from "../analytics/knowledge";
import liveStatsPack from "../assets/dota/6989/stats.json";
import { dotaIcon } from "./dotaIconSet";
import { HeroPortrait } from "../deck/HeroPortrait";
import { finalOwnDerivedStats } from "./heroCardModel";
import { postGameCopy } from "./copy";
import type { PostGameSummaryLayoutMode, PostGameSummaryView } from "./postGameSummaryModel";
import { fitTimelineRows, postGameDerivedStatPresentation, postGameSummaryLayoutMode } from "./postGameSummaryModel";
import type { StatsData } from "../stats/derivedStats";

const LIVE_STATS_DATA = liveStatsPack as unknown as StatsData;

function ItemIcon({ id, label, neutral = false }: { id: string | null; label: string; neutral?: boolean }) {
  if (!id) return <span className={`live-postgame-item is-unknown${neutral ? " is-neutral" : ""}`} title={`${label}: —`} aria-label={`${label}: —`}>—</span>;
  const icon = dotaIcon("item", id);
  return <span className={`live-postgame-item${neutral ? " is-neutral" : ""}`} title={`${label}: ${id}`} aria-label={`${label}: ${id}`}>
    {icon ? <img src={icon} alt="" /> : <span>{id.slice(5).replaceAll("_", " ")}</span>}
  </span>;
}

function ReplayStateMessage({ summary, onOpenReplaySettings }: {
  summary: PostGameSummaryView;
  onOpenReplaySettings?: () => void;
}) {
  const state = summary.replayState;
  return <>
    <strong>{state.label}</strong>
    {state.hint && <small className="live-postgame-state-hint">{state.hint}</small>}
    {state.settingsSection && onOpenReplaySettings && <a className="live-postgame-settings-link" href="#settings-section-replay-ai"
      onClick={event => { event.preventDefault(); onOpenReplaySettings(); }}>
      {state.settingsLinkLabel}
    </a>}
  </>;
}

export function PostGameSummary({ summary, locale, onClose, onOpenReplay, onOpenReplaySettings, onPickReplay, pickPending, pickFailed }: {
  summary: PostGameSummaryView;
  locale: "th" | "en";
  onClose: () => void;
  onOpenReplay: (matchKey?: string) => void;
  onOpenReplaySettings?: () => void;
  onPickReplay: () => void;
  pickPending: boolean;
  pickFailed: boolean;
}) {
  const text = postGameCopy(locale);
  const [tab, setTab] = useState<"summary" | "timeline" | "replay">("summary");
  const [copied, setCopied] = useState(false);
  const summaryRef = useRef<HTMLElement>(null);
  const [layoutMode, setLayoutMode] = useState<PostGameSummaryLayoutMode>("compact");
  const timelineRef = useRef<HTMLElement>(null);
  const [timelineVisibleCount, setTimelineVisibleCount] = useState(summary.recapEvents.length);
  const watcherFolder = summary.watcher.folder === "known"
    ? text.watcherFound
    : summary.watcher.folder === "not_found" ? text.watcherNotFound : text.watcherUnknown;
  const lastScan = summary.watcher.lastScanAtMs === null
    ? text.watcherUnknown
    : new Date(summary.watcher.lastScanAtMs).toLocaleTimeString(locale === "th" ? "th-TH" : "en-US");
  const derivedStats = summary.derivedStats.length > 0 ? summary.derivedStats : finalOwnDerivedStats(
    summary.heroHeader.heroId,
    summary.heroHeader.levelValue,
    summary.inventory.slots,
    LIVE_STATS_DATA,
  );
  const benchmarkFor = (key: string) => summary.heroHeader.benchmarkComparisons.find(metric => metric.key === key) ?? null;
  const heroMetrics = [
    { key: "kda", label: text.kda, value: summary.heroHeader.kda, comparison: benchmarkFor("kpm"), reason: null },
    { key: "lhdn", label: text.lastHitsDenies, value: summary.heroHeader.lastHitsDenies, comparison: benchmarkFor("lhpm"), reason: null },
    { key: "gpm", label: "GPM", value: summary.heroHeader.gpm, comparison: benchmarkFor("gpm"), reason: null },
    { key: "xpm", label: "XPM", value: summary.heroHeader.xpm, comparison: benchmarkFor("xpm"), reason: null },
    { key: "netWorth", label: text.netWorth, value: summary.heroHeader.netWorth, comparison: null, reason: summary.heroHeader.netWorthReason },
  ];
  const copyMatchId = async () => {
    const matchId = summary.resultBanner.matchId;
    if (!matchId || !globalThis.navigator?.clipboard?.writeText) return;
    try {
      await globalThis.navigator.clipboard.writeText(matchId);
      setCopied(true);
      window.setTimeout(() => setCopied(false), 1600);
    } catch {
      setCopied(false);
    }
  };

  useLayoutEffect(() => {
    const node = summaryRef.current;
    if (!node) return;
    const measure = () => {
      const rect = node.getBoundingClientRect();
      const next = postGameSummaryLayoutMode({ width: rect.width, height: rect.height });
      setLayoutMode(previous => previous === next ? previous : next);
    };
    measure();
    const observer = typeof ResizeObserver === "undefined" ? null : new ResizeObserver(measure);
    observer?.observe(node);
    window.addEventListener("resize", measure);
    return () => { observer?.disconnect(); window.removeEventListener("resize", measure); };
  }, []);

  useLayoutEffect(() => {
    if (tab !== "timeline") return;
    const node = timelineRef.current;
    if (!node) return;
    const measure = () => {
      const list = node.querySelector<HTMLOListElement>("ol[data-event-list]");
      if (!list) {
        setTimelineVisibleCount(0);
        return;
      }
      const rows = Array.from(list.querySelectorAll<HTMLElement>("li[data-event-row]"));
      const gap = Number.parseFloat(window.getComputedStyle(list).rowGap) || 0;
      const sectionGap = Number.parseFloat(window.getComputedStyle(node).rowGap) || 0;
      const chromeHeight = Array.from(node.children)
        .filter(child => child !== list)
        .reduce((total, child) => total + child.getBoundingClientRect().height, 0);
      const gaps = Math.max(0, node.children.length - 1) * sectionGap;
      const availableHeight = Math.max(0, node.clientHeight - chromeHeight - gaps);
      const visible = fitTimelineRows(rows.map(row => row.getBoundingClientRect().height), availableHeight, 22, gap);
      setTimelineVisibleCount(previous => previous === visible ? previous : visible);
    };
    measure();
    const observer = typeof ResizeObserver === "undefined" ? null : new ResizeObserver(measure);
    observer?.observe(node);
    window.addEventListener("resize", measure);
    return () => { observer?.disconnect(); window.removeEventListener("resize", measure); };
  }, [tab, summary.recapEvents, summary.recapDroppedCount]);

  return (
    <section ref={summaryRef} className="live-postgame-summary" aria-label={text.title} data-testid="live-postgame-summary" data-layout-mode={layoutMode}>
      <header className="live-postgame-heading">
        <div><small>{text.title}</small><h2>{text.matchComplete}</h2></div>
        <button type="button" onClick={onClose} aria-label={text.hide}>{text.hide}</button>
      </header>

      <section className="live-postgame-result-banner" aria-label={text.finalScore}>
        <div className="live-postgame-banner-main">
          <strong data-result={summary.result ?? "unknown"}>{summary.resultBanner.resultLabel}</strong>
          <div className="live-postgame-banner-meta">
            <span>{text.duration} {summary.resultBanner.duration}</span>
            {summary.resultBanner.gameMode && <span>{text.gameMode} {summary.resultBanner.gameMode}</span>}
            {summary.resultBanner.patch && <span>{text.patch} {summary.resultBanner.patch}</span>}
          </div>
          <div className="live-postgame-match-id"><span>{text.matchId}: {summary.resultBanner.matchId ?? "—"}</span>
            <button type="button" onClick={() => void copyMatchId()} disabled={!summary.resultBanner.matchId} aria-label={text.copyMatchId}>
              {copied ? text.copied : text.copyMatchId}
            </button>
          </div>
        </div>
        <div className="live-postgame-banner-score">
          <strong>{summary.resultBanner.finalScore}</strong>
        </div>
      </section>

      <section className="live-postgame-hero-header" aria-label={text.ownHero}
        title={derivedStats.map(stat => `${stat.label}: ${stat.value ?? "—"}`).join("\n") || undefined}>
        <div className="live-postgame-hero-overview">
          <HeroPortrait heroId={summary.heroHeader.heroId} size="lg" alt={summary.heroHeader.heroName} />
          <div className="live-postgame-hero-name"><strong>{summary.heroHeader.heroName}</strong><span>{text.level} {summary.heroHeader.level}</span></div>
        </div>
        <dl className="live-postgame-stat-tiles">
          {heroMetrics.map(metric => <div key={metric.key}>
            <dt>{metric.label}</dt>
            <dd>{metric.value}</dd>
            {metric.reason && <small className="live-postgame-stat-reason" title={text.netWorthAfterReplay}>{metric.reason}</small>}
            {metric.comparison && <span className="live-postgame-stat-comparison">{metric.comparison.label}: {metric.comparison.value} · P{Math.round(metric.comparison.percentile)}</span>}
          </div>)}
        </dl>
      </section>

      <div className="live-postgame-controls-row">
        <section className="live-postgame-inventory" aria-label={text.finalInventory}>
          <div><strong>{text.items}</strong>{summary.inventory.slots.map((item, index) => <ItemIcon key={`item-${index}`} id={item} label={`${text.items} ${index + 1}`} />)}</div>
          <div><strong>{text.backpack}</strong>{summary.inventory.backpack.map((item, index) => <ItemIcon key={`backpack-${index}`} id={item} label={`${text.backpack} ${index + 1}`} />)}</div>
          <div><strong>{text.neutralItem}</strong><ItemIcon id={summary.inventory.neutral} label={text.neutralItem} neutral /></div>
        </section>

        <nav className="live-postgame-tabs" aria-label={text.postGameTabs} role="tablist">
          {([ ["summary", text.summaryTab], ["timeline", text.timelineTab], ["replay", text.replayTab] ] as const).map(([id, label]) =>
            <button key={id} type="button" role="tab" aria-selected={tab === id} aria-controls={`live-postgame-panel-${id}`}
              id={`live-postgame-tab-${id}`} onClick={() => setTab(id)}>{label}</button>)}
        </nav>
      </div>

      <div className="live-postgame-tab-panel" role="tabpanel" id={`live-postgame-panel-${tab}`} aria-labelledby={`live-postgame-tab-${tab}`}>
        {tab === "summary" && <div className="live-postgame-stage-one">
          {summary.stage2.ready && summary.stage2.digest ? <>
            <div className="live-postgame-stage-two-heading">
              <h3>{text.replaySummary}</h3>
              <button type="button" onClick={() => onOpenReplay(summary.stage2.openMatchKey ?? undefined)}>{text.openAnalysis}</button>
            </div>
            <div className="live-postgame-stage-two">
              <PostGameMatchSummaryBlocks digest={summary.stage2.digest as any} ownHeroId={summary.heroHeader.heroId} compact={layoutMode === "compact"}
                onSeek={() => onOpenReplay(summary.stage2.openMatchKey ?? undefined)} />
            </div>
          </> : <>
            {derivedStats.length > 0 && <section className="live-postgame-derived">
              <h3>{text.derivedStats}</h3>
              <dl className="live-postgame-derived-stats">
                {derivedStats.map(stat => <div key={stat.label}>
                  {(() => {
                    const cell = postGameDerivedStatPresentation(stat, text.withoutTemporaryEffects);
                    return <><dt>{cell.label}</dt><dd title={cell.title}>{cell.value}</dd></>;
                  })()}
                </div>)}
              </dl>
            </section>}
            <div className="live-postgame-waiting" role="status" data-state={summary.replayParseState}>
              <ReplayStateMessage summary={summary} onOpenReplaySettings={onOpenReplaySettings} />
            </div>
          </>}
        </div>}

        {tab === "timeline" && <section ref={timelineRef} className="live-postgame-timeline" aria-label={text.recap}>
          {summary.recapEvents.length > 0 ? <ol data-event-list>{summary.recapEvents.slice(0, timelineVisibleCount).map((event, index) => <li data-event-row key={`${event.time}-${event.event}-${index}`}>
            <time>{event.time}</time><strong>{event.event}</strong><span>{event.kind}</span><small>{event.outcome}</small>
          </li>)}{timelineVisibleCount < summary.recapEvents.length && <li className="live-postgame-timeline-more">
            {text.moreTimelineEvents(summary.recapEvents.length - timelineVisibleCount)}
          </li>}</ol> : <p>{summary.recapAvailable ? text.noEvents : text.recapUnavailable}</p>}
          {summary.recapDroppedCount > 0 && <small>{text.droppedEvents(summary.recapDroppedCount)}</small>}
          {summary.recapSkippedCount > 0 && <small>{text.skippedRecapEvents(summary.recapSkippedCount)}</small>}
        </section>}

        {tab === "replay" && <section className="live-postgame-replay" aria-label={text.replaySteps}>
          <h3>{text.replaySteps}</h3>
          <ol className="live-postgame-steps">
            <li><h4>{text.stepDownload}</h4><p>{text.stepDownloadHelp}</p></li>
            <li>
              <h4>{text.stepFindParse}</h4>
              <p className="live-postgame-watcher"><strong>{text.watcher}:</strong> {text.watcherUnknown}</p>
              <p className="live-postgame-watcher"><strong>{text.replayFolder}:</strong> {watcherFolder}</p>
              <p className="live-postgame-watcher"><strong>{text.lastScan}:</strong> {lastScan}</p>
              <p className="live-postgame-parse-state" data-state={summary.replayParseState}>
                <ReplayStateMessage summary={summary} onOpenReplaySettings={onOpenReplaySettings} />
                {summary.replayFailureReason && summary.replayParseState === "failed" ? ` · ${summary.replayFailureReason}` : ""}
              </p>
              <button type="button" onClick={onPickReplay} disabled={pickPending}>{pickPending ? text.picking : text.manualPick}</button>
              {pickFailed && <p role="status">{text.pickFailed}</p>}
            </li>
            <li>
              <h4>{text.stepOpen}</h4>
              {summary.replay ? <>
                <dl>{summary.replay.headline.map(item => <div key={item.label}>
                  <dt>{item.label === "Deaths" || item.label === "จำนวนครั้งที่ตาย" ? text.deaths : text.netWorth}</dt><dd>{item.value}</dd>
                </div>)}</dl>
                <button type="button" onClick={() => onOpenReplay(summary.replay?.openMatchKey)}>{text.openAnalysis}</button>
              </> : <p><ReplayStateMessage summary={summary} onOpenReplaySettings={onOpenReplaySettings} /></p>}
            </li>
          </ol>
        </section>}
      </div>
    </section>
  );
}
