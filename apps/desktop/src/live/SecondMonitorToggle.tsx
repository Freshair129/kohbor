import { SECOND_MONITOR_SIDES, secondMonitorCopy, type SecondMonitorSide } from "./secondMonitorModel";
import "./headerControls.css";

const MONITOR_LABELS = {
  th: { off: "ปิด", left: "จอ 2 อยู่ซ้าย", right: "จอ 2 อยู่ขวา" },
  en: { off: "Off", left: "Monitor 2 on the left", right: "Monitor 2 on the right" },
} as const;

function MonitorGlyph({ side }: { side: SecondMonitorSide }) {
  if (side === "off") {
    return (
      <svg width="22" height="20" viewBox="0 0 24 20" aria-hidden="true" focusable="false">
        <rect x="2" y="1.5" width="20" height="13" rx="2" fill="var(--ground)" stroke="var(--line)" strokeWidth="1.3" />
        <path d="M9 17h6m-3-2.5V17" fill="none" stroke="var(--text-2)" strokeWidth="1.3" strokeLinecap="round" />
      </svg>
    );
  }

  const leftActive = side === "left";
  return (
    <svg width="24" height="20" viewBox="0 0 28 20" aria-hidden="true" focusable="false">
      <rect x="1.5" y="2" width="11.5" height="10.5" rx="1.5"
        fill={leftActive ? "var(--accent)" : "var(--ground)"} fillOpacity={leftActive ? ".32" : "1"}
        stroke={leftActive ? "var(--accent)" : "var(--line)"} strokeWidth="1.2" />
      <rect x="15" y="2" width="11.5" height="10.5" rx="1.5"
        fill={leftActive ? "var(--ground)" : "var(--accent)"} fillOpacity={leftActive ? "1" : ".32"}
        stroke={leftActive ? "var(--line)" : "var(--accent)"} strokeWidth="1.2" />
      <path d="M5 16h4m-2-3.5V16m11 0h4m-2-3.5V16" fill="none" stroke="var(--text-2)" strokeWidth="1.1" strokeLinecap="round" />
    </svg>
  );
}

/** Keyboard-reachable segmented control (UI-006 twin): plain buttons with aria-pressed. */
export function SecondMonitorToggle({ side, onChange, locale }: {
  side: SecondMonitorSide; onChange: (side: SecondMonitorSide) => void; locale: "th" | "en";
}) {
  const text = secondMonitorCopy(locale);
  const labels = MONITOR_LABELS[locale];
  return (
    <div className="sm-toggle sm-toggle-compact" role="group" aria-label={text.sideLabel} title={text.sideHint}>
      {SECOND_MONITOR_SIDES.map(option => (
        <button key={option} type="button" aria-label={labels[option]} title={labels[option]} aria-pressed={side === option} onClick={() => onChange(option)}>
          <MonitorGlyph side={option} />
        </button>
      ))}
    </div>
  );
}
