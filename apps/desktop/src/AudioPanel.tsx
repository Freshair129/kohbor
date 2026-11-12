import { invoke } from "@tauri-apps/api/core";
import { useCallback, useEffect, useRef, useState } from "react";
import { copy, reason, type Locale } from "./copy";
import { Chip } from "./Readiness";
import type { RuntimeStatus } from "./types";

export function AudioPanel({ status, locale }: { status: RuntimeStatus | null; locale: Locale }) {
  const text = copy(locale).audio;
  const [muted, setMuted] = useState(status?.media.reason === "AUDIO_MUTED");
  const [pending, setPending] = useState(false);
  const [failed, setFailed] = useState(false);
  const requestPending = useRef(false);

  useEffect(() => {
    if (status) setMuted(status.media.reason === "AUDIO_MUTED");
  }, [status?.media.reason]);

  const toggleMuted = useCallback(async () => {
    if (requestPending.current) return;
    requestPending.current = true;
    setPending(true);
    setFailed(false);
    try {
      const result = await invoke<{ muted: boolean }>("set_muted", { muted: !muted });
      if (typeof result?.muted !== "boolean") throw new Error("invalid set_muted response");
      setMuted(result.muted);
    } catch {
      setFailed(true);
    } finally {
      requestPending.current = false;
      setPending(false);
    }
  }, [muted]);

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
        void toggleMuted();
      }
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [toggleMuted]);

  const media = status?.media ?? null;
  const mediaTone = media === null ? "unknown" : media.state === "ready" ? "fresh" : media.state === "not_ready" ? "warn" : "off";
  const mediaState = media === null ? text.mediaState.unknown : text.mediaState[media.state];
  const reasonText = media === null ? text.statusUnknown : media.reason === null ? text.reasonNone : reason(media.reason, locale);

  return (
    <section className="card audio-panel" aria-labelledby="audio-h">
      <div className="card-head">
        <h2 id="audio-h">{text.title}</h2>
      </div>
      <div className="audio-readiness" aria-label={text.mediaName}>
        <span className="row-name">{text.mediaName}</span>
        <Chip tone={mediaTone} label={mediaState} />
        <span className="row-detail">{reasonText}</span>
      </div>
      <div className="audio-control">
        <button
          type="button"
          role="switch"
          aria-checked={muted}
          aria-keyshortcuts="Alt+M"
          aria-label={muted ? text.unmuteAction : text.muteAction}
          aria-busy={pending}
          disabled={pending}
          onClick={() => void toggleMuted()}
        >
          {pending ? text.pending : muted ? text.unmuteAction : text.muteAction}
        </button>
        <span className="muted small">Alt+M</span>
      </div>
      {failed && <p className="error" role="alert">{text.toggleFailed}</p>}
      <p className="audio-note">{text.muteWarning}</p>
      <p className="audio-note">{text.missingCriticalNote}</p>
    </section>
  );
}
