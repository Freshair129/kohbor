import type { Locale } from "../copy";
import type { AudioHistory, HistoryLoadState } from "../history";
import { deckCopy } from "./copy";
import { deriveEventFeed } from "./eventFeedModel";

export function EventFeed({ locale, history, state, hostNowMs }: {
  locale: Locale; history: AudioHistory | null; state: HistoryLoadState; hostNowMs: number | null;
}) {
  const view = deriveEventFeed(history, state, hostNowMs, locale);
  return (
    <section className="deck-event-panel" aria-labelledby="deck-events-heading">
      <header className="deck-card-heading"><h2 id="deck-events-heading">{deckCopy(locale).eventFeed}</h2></header>
      <ul className="deck-event-list" aria-label={deckCopy(locale).eventFeed}>
        {view.message && <li className="deck-event-message" role={view.failed ? "alert" : "status"}>{view.message}</li>}
        {view.rows.map((row, index) => (
          <li className="deck-event-row" key={`${row.record.instance_id}:${row.outcomeCode}:${row.record.decided_monotonic_ms}:${index}`}>
            <span className="deck-event-age mono" title={row.age === "—" ? (locale === "th" ? "ยังไม่ทราบเวลาปัจจุบันของระบบหลัก" : "Current host clock is unknown") : undefined}>{row.age}</span>
            <span className="deck-event-name" title={row.record.event_type ?? undefined}>{row.eventLabel}</span>
            <span className="deck-event-outcome" title={row.outcomeCode}>{row.outcomeLabel}</span>
            <span className="deck-class-badge">{row.audioClass}</span>
            {row.demo && <span className="history-demo-label">DEMO</span>}
          </li>
        ))}
      </ul>
    </section>
  );
}
