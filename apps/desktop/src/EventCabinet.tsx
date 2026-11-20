import { useEffect, useId, useState } from "react";
import { beginCabinetRefresh, cabinetMatchesPack, cabinetPlayBlock, cabinetSummary, groupCabinetBindings, type CabinetBinding, type CabinetLoadResult } from "./cabinet";
import { cabinetEventLabel, cabinetReason, copy, seconds, type Locale } from "./copy";
import { errorCode } from "./packs";
import { mediaAudition, mediaEventMap, subscribeAuditionMatch, type MediaAuditionResult } from "./runtime";

interface ActivePackRef {
  install_id: string;
  revision: number;
}

function resultText(result: MediaAuditionResult, locale: Locale): string {
  const audio = result.audio_started
    ? result.audio_ended
      ? `${locale === "th" ? "เสียงเริ่มและจบแล้ว" : "Audio started and ended"}${result.audio_end_reason && result.audio_end_reason !== "finished" ? ` (${result.audio_end_reason})` : ""}`
      : (locale === "th" ? "เสียงเริ่มแล้ว แต่ยังยืนยันการจบไม่ได้" : "Audio started; end was not confirmed")
    : (locale === "th" ? "ยืนยันการเริ่มเสียงไม่ได้" : "Audio start was not confirmed");
  const visual = result.visual_status === "shown"
    ? (locale === "th" ? "ภาพแบนเนอร์เริ่มแสดงแล้ว" : "Banner video started")
    : result.visual_status === "failed"
      ? `${locale === "th" ? "ภาพแบนเนอร์เล่นไม่สำเร็จ" : "Banner video failed"} (${result.visual_reason ?? "UNKNOWN"}${result.video_error_code === null ? "" : ` · ${result.video_error_code}`})`
      : result.visual_status === "unknown"
        ? `${locale === "th" ? "ยังยืนยันผลภาพแบนเนอร์ไม่ได้" : "Banner video result is unknown"} (${result.visual_reason ?? "UNKNOWN"})`
        : (locale === "th" ? "แพ็กนี้ไม่มีภาพแบนเนอร์" : "This variant has no banner");
  return `${audio} · ${visual}`;
}

function stateText(binding: CabinetBinding, locale: Locale, unknownText: string, unmappedText: string): string {
  switch (binding.state) {
    case "ready": return "";
    case "inherited":
      return locale === "th"
        ? `ใช้คลิประดับ ${binding.resolved_from_tier ?? "ทั่วไป"} · ${binding.reason}`
        : `Uses ${binding.resolved_from_tier === null ? "general" : `tier ${binding.resolved_from_tier}`} clip · ${binding.reason}`;
    case "unmapped": return `${unmappedText} · ${binding.reason}`;
    case "invalid_media": return `${locale === "th" ? "สื่อไม่ผ่านการตรวจ" : "Media failed checks"} · ${binding.reason}`;
    case "detector_unavailable":
      return `${locale === "th" ? "ตัวตรวจจับยังไม่พร้อม" : "Detector unavailable"} · ${binding.reason}`;
    case "not_voiced": return `${locale === "th" ? "ไม่มีเสียงประกาศ" : "No announcement audio"} · ${binding.reason}`;
    case "unknown": return `${unknownText} · ${binding.reason}`;
  }
}

function blockText(code: string, locale: Locale): string {
  if (code === "AUDITION_EVENT_UNAVAILABLE") return locale === "th" ? "เหตุการณ์นี้ไม่มีเสียงให้ทดลอง" : "This event has no auditionable voice";
  if (code === "AUDITION_MEDIA_INVALID") return locale === "th" ? "คลิปนี้ไม่ผ่านการตรวจสื่อ" : "This clip failed media checks";
  return cabinetReason(code, locale);
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
    return beginCabinetRefresh(mediaEventMap, setLoad);
  }, [activePack?.install_id, activePack?.revision, matchActive, matchSignal, refreshKey]);

  const loadedCabinet = load.state === "ok" ? load.cabinet : null;
  const packMatches = loadedCabinet !== null && cabinetMatchesPack(loadedCabinet, activePack);
  const currentCabinet = loadedCabinet === null
    ? null
    : packMatches
      ? loadedCabinet
      : {
        ...loadedCabinet,
        bindings: loadedCabinet.bindings.map((binding) => ({
          ...binding,
          state: "unknown" as const,
          reason: "MEDIA_STATE_UNKNOWN",
          resolved_from_tier: null,
          selection: null,
          variants: [],
          mapped: false,
          duration_ms: null,
          has_visual: false,
        })),
      };
  const phaseMatchActive = matchActive || matchSignal?.active === true;

  const audition = async (binding: CabinetBinding) => {
    const key = `${binding.event_id}:${binding.tier ?? "none"}`;
    if (!currentCabinet || cabinetPlayBlock(currentCabinet, binding, disabled, busyKey !== null, phaseMatchActive)) return;
    setBusyKey(key);
    setMessage(null);
    try {
      const result = await mediaAudition(binding.event_id, binding.tier);
      setMessage({ key, text: resultText(result, locale), error: !result.audio_started || result.visual_status === "failed" || result.visual_status === "unknown" });
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
      {load.state === "unreadable" && (
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
                        {binding.variants.length > 0 && binding.duration_ms !== null ? (
                          <>
                            <span className="event-cabinet-duration">{text.duration(seconds(binding.duration_ms, locale))}</span>
                            <span className="event-cabinet-visual">{binding.has_visual ? text.hasBanner : text.audioOnly}</span>
                            {binding.state !== "ready" && <span className="event-cabinet-reason">{stateText(binding, locale, text.stateUnknown, text.unmapped)}</span>}
                          </>
                        ) : (
                          <span className={binding.state === "unmapped" ? "event-cabinet-unmapped" : "event-cabinet-unknown"}>
                            {stateText(binding, locale, text.stateUnknown, text.unmapped)}
                          </span>
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
                            {blockText(reasonCode, locale)}
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
