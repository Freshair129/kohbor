import { SECOND_MONITOR_SIDES, secondMonitorCopy, type SecondMonitorSide } from "./secondMonitorModel";

/** Keyboard-reachable segmented control (UI-006 twin): plain buttons with aria-pressed. */
export function SecondMonitorToggle({ side, onChange, locale }: {
  side: SecondMonitorSide; onChange: (side: SecondMonitorSide) => void; locale: "th" | "en";
}) {
  const text = secondMonitorCopy(locale);
  return (
    <div className="sm-toggle" role="group" aria-label={text.sideLabel} title={text.sideHint}>
      <span className="sm-toggle-label">{text.sideLabel}</span>
      {SECOND_MONITOR_SIDES.map(option => (
        <button key={option} type="button" aria-pressed={side === option} onClick={() => onChange(option)}>
          {text.sideOptions[option]}
        </button>
      ))}
    </div>
  );
}
