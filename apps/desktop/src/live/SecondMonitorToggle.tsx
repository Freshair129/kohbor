import { SECOND_MONITOR_SIDES, secondMonitorCopy, type SecondMonitorSide } from "./secondMonitorModel";
import "./headerControls.css";

/** Keyboard-reachable segmented control (UI-006 twin): plain buttons with aria-pressed. */
export function SecondMonitorToggle({ side, onChange, locale }: {
  side: SecondMonitorSide; onChange: (side: SecondMonitorSide) => void; locale: "th" | "en";
}) {
  const text = secondMonitorCopy(locale);
  const compact = locale === "th" ? { off: "ปิด", left: "ซ้าย", right: "ขวา" } : { off: "Off", left: "Left", right: "Right" };
  return (
    <div className="sm-toggle sm-toggle-compact" role="group" aria-label={text.sideLabel} title={text.sideHint}>
      {SECOND_MONITOR_SIDES.map(option => (
        <button key={option} type="button" aria-label={text.sideOptions[option]} title={text.sideOptions[option]} aria-pressed={side === option} onClick={() => onChange(option)}>
          {compact[option]}
        </button>
      ))}
    </div>
  );
}
