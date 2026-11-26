import { postGameCopy } from "./copy";
import type { PostGameSummaryView, ReplayParseState } from "./postGameSummaryModel";

function parseStateLabel(state: ReplayParseState, text: ReturnType<typeof postGameCopy>): string {
  switch (state) {
    case "not_found": return text.stateNotFound;
    case "available": return text.stateAvailable;
    case "queued": return text.stateQueued;
    case "parsing": return text.stateParsing;
    case "ready": return text.stateReady;
    case "failed": return text.stateFailed;
    case "needs_pack": return text.stateNeedsPack;
    default: return text.watcherUnknown;
  }
}

export function PostGameSummary({ summary, locale, onClose, onOpenReplay, onPickReplay, pickPending, pickFailed }: {
  summary: PostGameSummaryView;
  locale: "th" | "en";
  onClose: () => void;
  onOpenReplay: (matchKey?: string) => void;
  onPickReplay: () => void;
  pickPending: boolean;
  pickFailed: boolean;
}) {
  const text = postGameCopy(locale);
  const recap = summary.recapEvents.slice(-4).reverse();
  const watcherFolder = summary.watcher.folder === "known"
    ? text.watcherFound
    : summary.watcher.folder === "not_found" ? text.watcherNotFound : text.watcherUnknown;
  const lastScan = summary.watcher.lastScanAtMs === null
    ? text.watcherUnknown
    : new Date(summary.watcher.lastScanAtMs).toLocaleTimeString(locale === "th" ? "th-TH" : "en-US");

  return (
    <section className="live-postgame-summary" aria-label={text.title} data-testid="live-postgame-summary">
      <header className="live-postgame-heading">
        <div><small>{text.title}</small><h2>{text.matchComplete}</h2></div>
        <button type="button" onClick={onClose} aria-label={text.hide}>{text.hide}</button>
      </header>
      <div className="live-postgame-result-row">
        <strong>{summary.result === "win" ? text.win : summary.result === "loss" ? text.loss : text.unknownResult}</strong>
        <span>{summary.heroName}</span><span>{text.duration} {summary.duration}</span>
      </div>
      <dl className="live-postgame-stats">
        <div><dt>{text.kda}</dt><dd>{summary.kda}</dd></div>
        <div><dt>{text.lastHitsDenies}</dt><dd>{summary.lastHitsDenies}</dd></div>
        <div><dt>{text.gpmXpm}</dt><dd>{summary.gpmXpm}</dd></div>
        <div><dt>{text.finalScore}</dt><dd>{summary.finalScore}</dd></div>
        {summary.derivedStats.map(stat => <div key={stat.label}><dt>{stat.label}</dt><dd>{stat.value}</dd></div>)}
      </dl>
      <section className="live-postgame-events" aria-label={text.recap}>
        <h3>{text.recap}</h3>
        {recap.length ? <ul>{recap.map((event, index) => <li key={`${event.time}-${event.event}-${index}`}>
          <span>{event.time}</span><strong>{event.event}</strong><small>{event.kind} · {event.outcome}</small>
        </li>)}</ul> : <p>{summary.recapAvailable ? text.noEvents : text.recapUnavailable}</p>}
        {summary.recapDroppedCount > 0 && <small>{text.droppedEvents(summary.recapDroppedCount)}</small>}
      </section>
      <section className="live-postgame-replay" aria-label={text.replaySteps}>
        <h3>{text.replaySteps}</h3>
        <ol className="live-postgame-steps">
          <li>
            <h4>{text.stepDownload}</h4>
            <p>{text.stepDownloadHelp}</p>
          </li>
          <li>
            <h4>{text.stepFindParse}</h4>
            <p className="live-postgame-watcher"><strong>{text.watcher}:</strong> {text.watcherUnknown}</p>
            <p className="live-postgame-watcher"><strong>{text.replayFolder}:</strong> {watcherFolder}</p>
            <p className="live-postgame-watcher"><strong>{text.lastScan}:</strong> {lastScan}</p>
            <p className="live-postgame-parse-state" data-state={summary.replayParseState}>
              {parseStateLabel(summary.replayParseState, text)}
              {summary.replayFailureReason && summary.replayParseState === "failed" ? ` · ${summary.replayFailureReason}` : ""}
            </p>
            <button type="button" onClick={onPickReplay} disabled={pickPending}>
              {pickPending ? text.picking : text.manualPick}
            </button>
            {pickFailed && <p role="status">{text.pickFailed}</p>}
          </li>
          <li>
            <h4>{text.stepOpen}</h4>
            {summary.replay ? <>
              <dl>{summary.replay.headline.map(item => <div key={item.label}>
                <dt>{item.label === "Deaths" || item.label === "จำนวนครั้งที่ตาย" ? text.deaths : text.netWorth}</dt><dd>{item.value}</dd>
              </div>)}</dl>
              <button type="button" onClick={() => onOpenReplay(summary.replay?.openMatchKey)}>{text.openAnalysis}</button>
            </> : <p>{summary.lastFinishedMatchAvailable ? text.waitingForReplay : text.noReplayKey}</p>}
          </li>
        </ol>
      </section>
    </section>
  );
}
