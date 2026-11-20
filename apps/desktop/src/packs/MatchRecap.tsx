import { useState } from "react";
import { eventLabel } from "../eventLabels";
import type { Locale } from "../copy";
import { matchRecapCopy } from "./matchRecap.copy";
import { formatGameClock, type MatchRecapSnapshot } from "./recapModel";

export interface MatchRecapProps {
  locale: Locale;
  recap: MatchRecapSnapshot;
  onClear: () => void | Promise<void>;
  clearDisabled?: boolean;
}

export function MatchRecap({ locale, recap, onClear, clearDisabled = false }: MatchRecapProps) {
  const copy = matchRecapCopy(locale);
  const [confirming, setConfirming] = useState(false);
  const [clearing, setClearing] = useState(false);
  const [clearFailed, setClearFailed] = useState(false);

  async function clearConfirmed() {
    setClearing(true);
    setClearFailed(false);
    try {
      await onClear();
      setConfirming(false);
    } catch {
      setClearFailed(true);
    } finally {
      setClearing(false);
    }
  }

  return (
    <section className="packs-match-recap" aria-labelledby="match-recap-title">
      <header className="packs-match-recap__header">
        <h2 id="match-recap-title">{copy.title}</h2>
        <button
          type="button"
          onClick={() => {
            setClearFailed(false);
            setConfirming(true);
          }}
          disabled={clearDisabled || recap.matches.length === 0 || confirming}
        >
          {copy.clear}
        </button>
      </header>

      {confirming && (
        <div className="packs-match-recap__confirm" role="group" aria-label={copy.confirmPrompt}>
          <p>{copy.confirmPrompt}</p>
          <button type="button" onClick={() => setConfirming(false)} disabled={clearing}>
            {copy.cancel}
          </button>
          <button type="button" onClick={() => void clearConfirmed()} disabled={clearing || clearDisabled}>
            {clearing ? copy.clearing : copy.confirm}
          </button>
        </div>
      )}

      {clearFailed && <p className="packs-match-recap__error" role="alert">{copy.clearFailed}</p>}
      {!recap.readable && <p className="packs-match-recap__error" role="alert">{copy.unreadable}</p>}
      {recap.readable && recap.matches.length === 0 && (
        <p className="packs-match-recap__empty" role="status">{copy.empty}</p>
      )}

      {recap.readable && recap.matches.map((match, index) => (
        <article className="packs-match-recap__match" key={match.match_id}>
          <h3>{copy.matchTitle(index)}</h3>
          {match.playedEvents.length === 0 ? (
            <p className="packs-match-recap__empty-match">{copy.noPlayed}</p>
          ) : (
            <ul className="packs-match-recap__events">
              {match.playedEvents.map((event) => (
                <li className="packs-match-recap__event" key={event.playback_id ?? event.instance_id}>
                  <time className="packs-match-recap__clock">{formatGameClock(event.game_clock_seconds)}</time>
                  <div className="packs-match-recap__event-detail">
                    <span className="packs-match-recap__event-name">
                      {eventLabel(event.event_type, locale)}
                      {event.tier === null ? "" : ` · ${copy.tier(event.tier)}`}
                    </span>
                    <span className="packs-match-recap__provenance">
                      {event.variant_id === null ? "" : copy.variant(event.variant_id)}
                      {event.clip_id === null ? "" : ` · ${copy.clip(event.clip_id)}`}
                      {event.selection === null ? "" : ` · ${copy.selection(event.selection)}`}
                    </span>
                    <span className="packs-match-recap__result">
                      {copy.played} · {copy.end[event.playback_end ?? "unknown"]}
                    </span>
                  </div>
                </li>
              ))}
            </ul>
          )}
          {match.droppedCount > 0 && (
            <p className="packs-match-recap__dropped">{copy.dropped(match.droppedCount)}</p>
          )}
        </article>
      ))}

      {recap.readable && recap.skipped > 0 && (
        <p className="packs-match-recap__error" role="status">{copy.skipped(recap.skipped)}</p>
      )}
      <p className="packs-match-recap__footnote">{copy.footnote}</p>
    </section>
  );
}
