import { useEffect, useRef, useState } from "react";
import { copy, type Locale } from "./copy";
import { TransientNotice, type AudioVolumeEvent } from "./audioPrefs";
import { useAudioPrefs } from "./useAudioPrefs";

const TOAST_DURATION_MS = 1_500;

export function VolumeToast({ locale }: { locale: Locale }) {
  const { volumeEvent } = useAudioPrefs();
  const [visibleEvent, setVisibleEvent] = useState<AudioVolumeEvent | null>(null);
  const notice = useRef<TransientNotice<AudioVolumeEvent> | null>(null);
  const lastSeenSequence = useRef(volumeEvent?.sequence ?? 0);
  const text = copy(locale).audio.volume;

  useEffect(() => {
    const controller = new TransientNotice<AudioVolumeEvent>(setVisibleEvent, TOAST_DURATION_MS);
    notice.current = controller;
    return () => {
      controller.dispose();
      notice.current = null;
    };
  }, []);

  useEffect(() => {
    if (!volumeEvent || volumeEvent.sequence <= lastSeenSequence.current) return;
    lastSeenSequence.current = volumeEvent.sequence;
    notice.current?.show(volumeEvent.event);
  }, [volumeEvent?.sequence]);

  if (!visibleEvent) return null;
  return (
    <div className="volume-toast" role="status" aria-live="polite">
      <span className="volume-toast-bar" aria-hidden="true">
        <span style={{ width: `${visibleEvent.percent}%` }} />
      </span>
      <span>{text.toastMessage(visibleEvent.percent)}</span>
    </div>
  );
}
