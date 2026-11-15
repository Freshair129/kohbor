import { useId } from "react";
import { copy, type Locale } from "./copy";
import { volumeControlModel } from "./audioPrefs";
import { useAudioPrefs, audioPrefsStore } from "./useAudioPrefs";

export function VolumeControl({ locale, compact = false }: { locale: Locale; compact?: boolean }) {
  const sliderId = useId();
  const hotkeysId = useId();
  const labelId = useId();
  const prefs = useAudioPrefs();
  const text = copy(locale).audio.volume;
  const model = volumeControlModel(prefs.prefs, prefs.draftPercent, text);

  return (
    <div className={`volume-control${compact ? " volume-control-compact" : ""}`}>
      <div className="volume-control-label">
        <span id={labelId}>{text.announcerVolume}</span>
        <output aria-live="polite" role={model.unknown ? "status" : undefined}>{model.readout}</output>
      </div>
      {model.percent === null ? (
        null
      ) : (
        <input
          id={sliderId}
          className="volume-slider"
          type="range"
          min={0}
          max={100}
          step={5}
          value={model.percent}
          aria-labelledby={labelId}
          onChange={(event) => audioPrefsStore.setVolume(Number(event.currentTarget.value))}
        />
      )}
      {model.mutedMessage && <p className="volume-muted" role="status">{model.mutedMessage}</p>}
      {model.hotkeysEnabled === null ? (
        <p className="volume-unknown" role="status">{text.hotkeysUnknown}</p>
      ) : (
        <label className="volume-hotkeys" htmlFor={hotkeysId}>
          <input
            id={hotkeysId}
            type="checkbox"
            role="switch"
            aria-checked={model.hotkeysEnabled}
            aria-describedby={`${hotkeysId}-hint`}
            aria-keyshortcuts="Alt+ArrowUp Alt+ArrowDown"
            checked={model.hotkeysEnabled}
            disabled={prefs.hotkeysPending}
            onChange={(event) => audioPrefsStore.setHotkeys(event.currentTarget.checked)}
          />
          <span>{text.hotkeysLabel}</span>
          <strong>{model.hotkeysEnabled ? text.hotkeysOn : text.hotkeysOff}</strong>
        </label>
      )}
      <p id={`${hotkeysId}-hint`} className="volume-hint">{text.hotkeysHint}</p>
      {model.hotkeysRegistrationMessage && <p className="volume-warning" role="status">{model.hotkeysRegistrationMessage}</p>}
      {prefs.error === "volume_failed" && <p className="volume-error" role="alert">{text.changeFailed}</p>}
      {prefs.error === "hotkeys_failed" && <p className="volume-error" role="alert">{text.hotkeysChangeFailed}</p>}
    </div>
  );
}
