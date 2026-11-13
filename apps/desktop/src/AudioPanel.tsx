import { invoke } from "@tauri-apps/api/core";
import { useCallback, useEffect, useRef, useState } from "react";
import { copy, mediaHint, mediaReason, type Locale } from "./copy";
import { deriveMuted, mediaTone } from "./mediaStatus";
import { Chip } from "./Readiness";
import type { RuntimeStatus } from "./types";

export interface AudioMuteState {
  muted: boolean | null;
  pending: boolean;
  failed: boolean;
  setMutedTo: (target: boolean) => Promise<void>;
}

/** Shared state keeps the always-visible header control and detail panel in sync. */
export function useAudioMute(status: RuntimeStatus | null): AudioMuteState {
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
        // An unconfirmed state cannot be toggled safely; explicit controls remain available.
        if (muted !== null) void setMutedTo(!muted);
      }
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [muted, setMutedTo]);

  return { muted, pending, failed, setMutedTo };
}

export function AudioMuteControl({
  control,
  locale,
  compact = false,
}: {
  control: AudioMuteState;
  locale: Locale;
  compact?: boolean;
}) {
  const text = copy(locale).audio;
  return (
    <div className={compact ? "shell-mute-control" : "audio-mute-control"} role="group" aria-label={text.title}>
      {control.muted === null ? (
        <>
          <button type="button" aria-busy={control.pending} disabled={control.pending} onClick={() => void control.setMutedTo(true)}>
            {text.muteAction}
          </button>
          <button type="button" aria-busy={control.pending} disabled={control.pending} onClick={() => void control.setMutedTo(false)}>
            {text.unmuteAction}
          </button>
        </>
      ) : (
        <>
          <button
            type="button"
            role="switch"
            aria-checked={control.muted}
            aria-keyshortcuts="Alt+M"
            aria-label={control.muted ? text.unmuteAction : text.muteAction}
            title="Alt+M"
            aria-busy={control.pending}
            disabled={control.pending}
            onClick={() => void control.setMutedTo(control.muted !== true)}
          >
            {control.failed ? text.toggleFailed : control.pending ? text.pending : control.muted ? text.unmuteAction : text.muteAction}
          </button>
          {!compact && <span className="muted small">Alt+M</span>}
        </>
      )}
    </div>
  );
}

export function AudioPanel({ status, locale, mute }: { status: RuntimeStatus | null; locale: Locale; mute: AudioMuteState }) {
  const text = copy(locale).audio;
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

      {mute.muted === true && (
        <p className="banner banner-warn" role="status">
          {text.mutedBanner}
        </p>
      )}
      <AudioMuteControl control={mute} locale={locale} />
      {mute.muted === null && <p className="audio-note">{text.muteUnknown}</p>}
      {mute.failed && <p className="error" role="alert">{text.toggleFailed}</p>}
      <p className="audio-note">{text.muteWarning}</p>
      {readinessNote && <p className="audio-note">{readinessNote}</p>}
    </section>
  );
}
