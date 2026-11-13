import { invoke } from "@tauri-apps/api/core";
import { useCallback, useEffect, useRef, useState } from "react";
import { copy, mediaHint, mediaReason, type Locale } from "./copy";
import { deriveMuted, mediaTone } from "./mediaStatus";
import { Chip } from "./Readiness";
import type { RuntimeStatus } from "./types";

export function AudioPanel({ status, locale }: { status: RuntimeStatus | null; locale: Locale }) {
  const text = copy(locale).audio;
  // null = cannot be confirmed (audio engine unavailable and nothing acknowledged yet).
  const [muted, setMuted] = useState<boolean | null>(() => deriveMuted(status?.media ?? null, null));
  const [pending, setPending] = useState(false);
  const [failed, setFailed] = useState(false);
  const requestPending = useRef(false);

  useEffect(() => {
    if (status) setMuted((previous) => deriveMuted(status.media, previous));
  }, [status?.media.state, status?.media.reason]);

  const setMutedTo = useCallback(async (target: boolean) => {
    if (requestPending.current) return;
    requestPending.current = true;
    setPending(true);
    setFailed(false);
    try {
      const result = await invoke<{ muted: boolean }>("set_muted", { muted: target });
      if (typeof result?.muted !== "boolean") throw new Error("invalid set_muted response");
      setMuted(result.muted);
    } catch {
      setFailed(true);
    } finally {
      requestPending.current = false;
      setPending(false);
    }
  }, []);

  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      if (
        event.altKey &&
        !event.ctrlKey &&
        !event.metaKey &&
        !event.shiftKey &&
        event.key.toLowerCase() === "m" &&
        !event.repeat
      ) {
        event.preventDefault();
        // With an unconfirmed state a toggle could do the opposite of what was meant, so it does nothing.
        if (muted !== null) void setMutedTo(!muted);
      }
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [muted, setMutedTo]);

  const media = status?.media ?? null;
  const mediaState = media === null ? text.mediaState.unknown : text.mediaState[media.state];
  // A ready state has no reason by design; a missing reason on any other state is a host gap worth saying so.
  const reasonText =
    media === null
      ? text.statusUnknown
      : media.reason === null
        ? media.state === "ready" ? text.readyReason : text.reasonNone
        : mediaReason(media.reason, locale);
  const hint = media === null ? null : mediaHint(media.reason, locale);
  const readinessNote = media === null ? null : media.state === "ready" ? text.readyNote : text.missingCriticalNote;

  return (
    <section className="card audio-panel" aria-labelledby="audio-h">
      <div className="card-head">
        <h2 id="audio-h">{text.title}</h2>
      </div>
      <div className="audio-readiness" aria-label={text.mediaName}>
        <span className="row-name">{text.mediaName}</span>
        <Chip tone={mediaTone(media)} label={mediaState} />
        <span className="row-detail">{reasonText}</span>
      </div>
      {hint && (
        <p className="audio-note">
          <strong>{text.hintLabel}:</strong> {hint}
        </p>
      )}

      {muted === true && (
        <p className="banner banner-warn" role="status">
          {text.mutedBanner}
        </p>
      )}
      <div className="audio-control">
        {muted === null ? (
          <>
            <button type="button" aria-busy={pending} disabled={pending} onClick={() => void setMutedTo(true)}>
              {text.muteAction}
            </button>
            <button type="button" aria-busy={pending} disabled={pending} onClick={() => void setMutedTo(false)}>
              {text.unmuteAction}
            </button>
          </>
        ) : (
          <>
            <button
              type="button"
              role="switch"
              aria-checked={muted}
              aria-keyshortcuts="Alt+M"
              aria-label={muted ? text.unmuteAction : text.muteAction}
              aria-busy={pending}
              disabled={pending}
              onClick={() => void setMutedTo(!muted)}
            >
              {pending ? text.pending : muted ? text.unmuteAction : text.muteAction}
            </button>
            <span className="muted small">Alt+M</span>
          </>
        )}
      </div>
      {muted === null && <p className="audio-note">{text.muteUnknown}</p>}
      {failed && <p className="error" role="alert">{text.toggleFailed}</p>}
      <p className="audio-note">{text.muteWarning}</p>
      {readinessNote && <p className="audio-note">{readinessNote}</p>}
    </section>
  );
}
