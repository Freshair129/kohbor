import { useEffect, useRef, useState, type ReactNode } from "react";
import { copy, type Locale } from "../copy";
import type { AnalyticsReplayDetailsRoute } from "../analytics/analyticsModel";
import { CoachPanel } from "./CoachPanel";
import { MatchViewPanel } from "./MatchViewPanel";
import { matchViewerCopy } from "./matchViewerCopy.mjs";
import { MAX_REPORT_BYTES, readPostMatchReplay, type PostMatchReplay } from "./report";
import { mountReplay } from "./viewer.mjs";
import markup from "./viewer.html?raw";
import style from "./viewer.css?raw";

export function ReplayPanel({ locale, available, detail, onBack }: {
  locale: Locale;
  available: boolean;
  detail?: AnalyticsReplayDetailsRoute | null;
  onBack?: () => void;
}) {
  const text = matchViewerCopy[locale].panel;
  if (detail) return (
    <div className="shell-screen shell-replay-detail" style={{ display: "grid", gridTemplateRows: "auto minmax(0, 1fr)", gap: 8, minWidth: 0, minHeight: 0, overflow: "hidden" }}>
      <header className="shell-replay-detail__header">
        <button type="button" onClick={onBack}>{copy(locale).analyticsPlayer.back}</button>
      </header>
      <MatchViewPanel locale={locale} initialBundle={detail.bundle} initialMatchId={detail.matchId}
        initialTime={detail.initialTime} detailOnly />
    </div>
  );
  return available ? <LocalReplay locale={locale} /> : (
    <div className="shell-screen" style={{ overflow: "auto" }}>
      <section className="card">
        <h1>{text.title}</h1>
        <p role="status">{text.availability}</p>
      </section>
    </div>
  );
}

type ReplayView = "report" | "match";

function LocalReplay({ locale }: { locale: Locale }) {
  const text = matchViewerCopy[locale].panel;
  const [view, setView] = useState<ReplayView>("report");
  const views: [ReplayView, string][] = [
    ["report", text.reportTab],
    ["match", text.matchTab],
  ];
  const switcher = (
    <div role="group" aria-label={text.viewGroup} style={{ display: "flex", gap: 8 }}>
      {views.map(([id, label]) => (
        <button key={id} type="button" aria-pressed={view === id} onClick={() => setView(id)}>{label}</button>
      ))}
    </div>
  );
  return view === "match" ? (
    <div className="shell-screen" style={{ overflow: "auto" }}>
      <section className="card">
        <h1>{text.title}</h1>
        {switcher}
      </section>
      <MatchViewPanel locale={locale} />
    </div>
  ) : <ReportReplay locale={locale} switcher={switcher} />;
}

function ReportReplay({ locale, switcher }: { locale: Locale; switcher: ReactNode }) {
  const text = matchViewerCopy[locale].panel;
  const [report, setReport] = useState<PostMatchReplay | null>(null);
  const [invalid, setInvalid] = useState(false);
  const [reading, setReading] = useState(false);
  const target = useRef<HTMLDivElement>(null);
  const request = useRef(0);

  useEffect(() => () => { request.current++; }, []);
  useEffect(() => {
    const frame = target.current;
    if (!report || !frame) return;
    const root = frame.shadowRoot ?? frame.attachShadow({ mode: "open" });
    return mountReplay(root, report, { locale, markup, style });
  }, [report, locale]);

  async function openFile(file: File) {
    const ticket = ++request.current;
    setReading(true);
    setInvalid(false);
    let next: PostMatchReplay | null = null;
    if (file.size <= MAX_REPORT_BYTES) {
      try {
        next = readPostMatchReplay(JSON.parse(await file.text()));
      } catch {
        // An unreadable or malformed file follows the same input-error path.
      }
    }
    if (ticket !== request.current) return;
    setReading(false);
    if (next) setReport(next);
    else setInvalid(true);
  }

  return (
    <div className="shell-screen" style={{ overflow: "auto" }}>
      <section className="card">
        <h1>{text.title}</h1>
        {switcher}
        <label>
          {text.openReport}
          <input type="file" accept=".json,application/json" onChange={event => {
            const file = event.currentTarget.files?.[0];
            event.currentTarget.value = "";
            if (file) void openFile(file);
          }} />
        </label>
        {reading && <p role="status">{text.readingReport}</p>}
        {invalid && <p role="alert">{text.invalidReport}</p>}
      </section>
      {report && <CoachPanel report={report} locale={locale} />}
      <div ref={target} />
    </div>
  );
}
