import { useCallback, useEffect, useRef, useState } from "react";
import { copy, packReason, type Locale } from "./copy";
import {
  formatElapsed,
  HISTORY_LIMIT,
  isDemoOutcomeRecord,
  outcomeTone,
  parseAudioHistory,
  type AudioHistory as AudioHistoryData,
  type OutcomeTone,
} from "./history";
import { Chip, type Tone } from "./Readiness";
import { audioHistory } from "./runtime";

const POLL_MS = 2_000;

type LoadState = "loading" | "ok" | "unreadable" | "failed";

const CHIP_TONE: Record<OutcomeTone, Tone> = {
  played: "fresh",
  blocked: "warn",
  silent: "off",
};

function signature(history: AudioHistoryData): string {
  const newest = history.records[0];
  return `${history.records.length}:${history.skipped}:${newest?.instance_id ?? ""}:${newest?.outcome ?? ""}`;
}

export function AudioHistory({ locale, disabled = false }: { locale: Locale; disabled?: boolean }) {
  const text = copy(locale).history;
  const [history, setHistory] = useState<AudioHistoryData | null>(null);
  const [state, setState] = useState<LoadState>("loading");
  const [errorCode, setErrorCode] = useState<string | null>(null);
  const alive = useRef(true);
  const seq = useRef(0);
  const lastSignature = useRef<string | null>(null);

  useEffect(() => {
    alive.current = true;
    return () => {
      alive.current = false;
    };
  }, []);

  const load = useCallback(async () => {
    const mine = ++seq.current;
    try {
      const parsed = parseAudioHistory(await audioHistory(HISTORY_LIMIT));
      if (!alive.current || mine !== seq.current) return;
      if (!parsed) {
        lastSignature.current = null;
        setHistory(null);
        setState("unreadable");
        return;
      }
      const sig = signature(parsed);
      if (sig !== lastSignature.current) {
        lastSignature.current = sig;
        setHistory(parsed);
      }
      setState("ok");
      setErrorCode(null);
    } catch (e) {
      if (!alive.current || mine !== seq.current) return;
      // Keep the last rows on screen, marked as not current, instead of presenting them as live.
      setState("failed");
      setErrorCode(typeof e === "string" ? e : e instanceof Error ? e.message : "UNKNOWN");
    }
  }, []);

  useEffect(() => {
    if (disabled) return;
    let cancelled = false;
    let running = false;
    let timer: number | undefined;
    const tick = async () => {
      if (running || cancelled) return;
      running = true;
      window.clearTimeout(timer);
      try {
        await load();
      } finally {
        running = false;
      }
      // A hidden window does not poll; visibilitychange resumes it.
      if (!cancelled && !document.hidden) timer = window.setTimeout(() => void tick(), POLL_MS);
    };
    void tick();
    const onVisibility = () => {
      if (!document.hidden) void tick();
    };
    document.addEventListener("visibilitychange", onVisibility);
    return () => {
      cancelled = true;
      window.clearTimeout(timer);
      document.removeEventListener("visibilitychange", onVisibility);
    };
  }, [disabled, load]);

  const outdated = state === "failed" || disabled;
  const records = history?.records ?? [];

  return (
    <section className="card history-panel" aria-labelledby="history-h">
      <div className="card-head">
        <h2 id="history-h">{text.title}</h2>
        <button type="button" onClick={() => void load()} disabled={disabled}>
          {text.refresh}
        </button>
      </div>
      <p className="audio-note">{text.note}</p>

      {state === "loading" && (
        <p className="muted" role="status">
          {text.loading}
        </p>
      )}
      {state === "unreadable" && (
        <p className="error" role="alert">
          {text.unreadable}
        </p>
      )}
      {state === "failed" && (
        <p className="error" role="alert">
          {text.failed} — {packReason(errorCode, locale)}
        </p>
      )}

      {history && records.length === 0 && state !== "loading" && <p className="muted">{text.empty}</p>}

      {history && records.length > 0 && (
        <div className={outdated ? "history-scroll is-outdated" : "history-scroll"}>
          <table className="history-table">
            <thead>
              <tr>
                <th scope="col" title={text.timeNote}>
                  {text.timeHeader}
                </th>
                <th scope="col">{text.eventHeader}</th>
                <th scope="col">{text.outcomeHeader}</th>
                <th scope="col">{text.packHeader}</th>
              </tr>
            </thead>
            <tbody>
              {records.map((record) => {
                const outcome = text.outcomes[record.outcome];
                const tone = outcomeTone(record.outcome);
                return (
                  <tr key={`${record.instance_id}:${record.outcome}:${record.decided_monotonic_ms}`}>
                    <td className="mono" title={`${record.decided_monotonic_ms} ms`}>
                      {formatElapsed(record.decided_monotonic_ms)}
                    </td>
                    <td>
                      {isDemoOutcomeRecord(record) ? <span className="history-demo-label">DEMO</span> : null}
                      <span className="mono">{record.event_type ?? text.voiceEvent}</span>
                      <span className="muted small">
                        {" "}
                        · {text.classes[record.audio_class]} · {text.session(record.session_epoch)}
                      </span>
                    </td>
                    <td>
                      <Chip tone={CHIP_TONE[tone]} label={outcome.label} />
                      <span className="muted small history-detail">{outcome.detail}</span>
                    </td>
                    <td className="muted small">
                      {record.package_revision === null ? text.noRevision : text.revision(record.package_revision)}
                    </td>
                  </tr>
                );
              })}
            </tbody>
          </table>
        </div>
      )}

      {history && records.length > 0 && <p className="muted small">{text.showing(records.length)}</p>}
      {history && history.skipped > 0 && <p className="error small">{text.skipped(history.skipped)}</p>}
    </section>
  );
}
