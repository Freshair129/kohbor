import { useEffect, useId, useState } from "react";
import { beginCabinetRefresh, cabinetMatchesPack, cabinetPlayBlock, cabinetSummary, groupCabinetBindings, type CabinetBinding, type CabinetLoadResult } from "./cabinet";
import { cabinetEventLabel, cabinetReason, copy, seconds, type Locale } from "./copy";
import { errorCode } from "./packs";
import { mediaAudition, mediaCabinet, subscribeAuditionMatch } from "./runtime";

interface ActivePackRef {
  install_id: string;
  revision: number;
}

export function EventCabinet({
  locale,
  activePack,
  refreshKey,
  disabled = false,
  matchActive = false,
}: {
  locale: Locale;
  activePack: ActivePackRef | null | undefined;
  refreshKey: number;
  disabled?: boolean;
  matchActive?: boolean;
}) {
  const text = copy(locale).packs.cabinet;
  const id = useId();
  const [load, setLoad] = useState<CabinetLoadResult | { state: "loading" }>({ state: "loading" });
  const [matchSignal, setMatchSignal] = useState<{ active: boolean | null } | null>(null);
  const [busyKey, setBusyKey] = useState<string | null>(null);
  const [message, setMessage] = useState<{ key: string | null; text: string; error: boolean } | null>(null);

  useEffect(() => {
    let disposed = false;
    // Start listening before fetching the initial snapshot. GSI phase can
    // change even when the session status has no verified match epoch.
    const subscription = subscribeAuditionMatch((active) => {
      if (!disposed) setMatchSignal({ active });
    });
    void subscription.then(() => {
      if (!disposed) setMatchSignal((current) => current ?? { active: null });
    }, (error) => {
      if (!disposed) setLoad({ state: "failed", error });
    });
    return () => {
      disposed = true;
      void subscription.then((unlisten) => unlisten(), () => {});
    };
  }, []);

  useEffect(() => {
    if (matchSignal === null) return;
    setLoad({ state: "loading" });
    return beginCabinetRefresh(mediaCabinet, setLoad);
  }, [activePack?.install_id, activePack?.revision, matchActive, matchSignal, refreshKey]);

  const currentCabinet = load.state === "ok" && cabinetMatchesPack(load.cabinet, activePack) ? load.cabinet : null;
  const phaseMatchActive = matchActive || matchSignal?.active === true;

  const audition = async (binding: CabinetBinding) => {
    const key = `${binding.event_id}:${binding.tier ?? "none"}`;
    if (!currentCabinet || cabinetPlayBlock(currentCabinet, binding, disabled, busyKey !== null, phaseMatchActive)) return;
    setBusyKey(key);
    setMessage(null);
    try {
      await mediaAudition(binding.event_id, binding.tier);
      setMessage({ key, text: text.started, error: false });
    } catch (failure) {
      setMessage({ key, text: cabinetReason(errorCode(failure), locale), error: true });
    } finally {
      setBusyKey(null);
    }
  };

  const summary = currentCabinet ? cabinetSummary(currentCabinet) : null;
  const groups = currentCabinet ? groupCabinetBindings(currentCabinet.bindings) : [];
  const anyMatchActive = phaseMatchActive || currentCabinet?.match_active === true;

  return (
    <section className="event-cabinet" aria-labelledby={`${id}-heading`}>
      <div className="event-cabinet-heading">
        <h3 id={`${id}-heading`}>{text.title}</h3>
        <strong className="event-cabinet-summary" aria-live="polite">
          {summary ? text.summary(summary.mapped, summary.total) : text.summaryUnknown}
        </strong>
      </div>

      {load.state === "loading" && <p className="muted small" role="status">{text.loading}</p>}
      {((load.state === "ok" && currentCabinet === null) || load.state === "unreadable") && (
        <p className="error small" role="status">{text.unknown}</p>
      )}
      {load.state === "failed" && (
        <p className="error small" role="status">
          {text.unknown} · {cabinetReason(errorCode(load.error), locale)}
        </p>
      )}
      {anyMatchActive && <p className="event-cabinet-reason">{text.reasons.AUDITION_MATCH_ACTIVE}</p>}

      {currentCabinet && (
        <>
          <p className="event-cabinet-active" role="status">
            {currentCabinet.active
              ? <>{text.activePack}: <strong>{currentCabinet.active.package_id}</strong> · <span className="mono">{currentCabinet.active.install_id}</span></>
              : text.noActivePack}
          </p>
          <p className="muted small">{text.note}</p>
          <div className="event-cabinet-groups" aria-label={text.title}>
            {groups.map((group) => (
              <article className="event-cabinet-group" key={group.eventId}>
                <h4>{cabinetEventLabel(group.eventId, locale)}</h4>
                <div className="event-cabinet-tiles">
                  {group.bindings.map((binding) => {
                    const key = `${binding.event_id}:${binding.tier ?? "none"}`;
                    const reasonCode = cabinetPlayBlock(currentCabinet, binding, disabled, busyKey !== null, phaseMatchActive);
                    const tileId = `${id}-${binding.event_id}-${binding.tier ?? "none"}`;
                    return (
                      <div
                        className={`event-cabinet-tile${binding.state === "unmapped" ? " is-unmapped" : ""}${binding.state === "unknown" ? " is-unknown" : ""}`}
                        key={key}
                      >
                        <span className="event-cabinet-tier">
                          {binding.tier === null ? text.untiered : text.tier(binding.tier)}
                        </span>
                        {binding.state === "ready" && binding.duration_ms !== null ? (
                          <>
                            <span className="event-cabinet-duration">{text.duration(seconds(binding.duration_ms, locale))}</span>
                            <span className="event-cabinet-visual">{binding.has_visual ? text.hasBanner : text.audioOnly}</span>
                          </>
                        ) : binding.state === "unmapped" ? (
                          <span className="event-cabinet-unmapped">{text.unmapped}</span>
                        ) : (
                          <span className="event-cabinet-unknown">{text.stateUnknown}</span>
                        )}
                        {binding.state !== "unmapped" && (
                          <button
                            type="button"
                            className="event-cabinet-play"
                            aria-label={`${text.play}: ${cabinetEventLabel(group.eventId, locale)}, ${binding.tier === null ? text.untiered : text.tier(binding.tier)}`}
                            disabled={reasonCode !== null}
                            aria-describedby={reasonCode ? `${tileId}-reason` : undefined}
                            aria-busy={busyKey === key}
                            onClick={() => void audition(binding)}
                          >
                            {busyKey === key ? text.playing : text.play}
                          </button>
                        )}
                        {reasonCode && binding.state !== "unmapped" && (
                          <span className="event-cabinet-reason" id={`${tileId}-reason`} role="status">
                            {cabinetReason(reasonCode, locale)}
                          </span>
                        )}
                        {message?.key === key && (
                          <span className={message.error ? "event-cabinet-result error" : "event-cabinet-result"} role="status">
                            {message.text}
                          </span>
                        )}
                      </div>
                    );
                  })}
                </div>
              </article>
            ))}
          </div>
        </>
      )}
    </section>
  );
}
