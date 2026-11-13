import { liveCopy, type LiveLocale } from "./copy";
import { dangerLabel, dangerTone, litDangerLeds } from "./meter";
import { riskTag } from "./state";
import type { RiskState } from "./types";

export function DangerMeter({ state, locale, compact = false }: { state: RiskState | null; locale: LiveLocale; compact?: boolean }) {
  const level = state?.meter ?? null;
  const label = dangerLabel(level, locale);
  const tone = dangerTone(level);
  const source = state ? riskTag(state) : null;
  const lit = new Set(litDangerLeds(level));

  return (
    <div className={`live-danger live-danger-${tone}${compact ? " live-danger-compact" : ""}`} aria-label={`${liveCopy(locale).danger}: ${label}`}>
      <span className="live-danger-leds" aria-hidden="true">
        {[0, 1, 2, 3].map((led) => (
          <i
            key={led}
            className={`live-danger-led${lit.has(led) ? ` live-danger-led-${tone}` : ""}`}
          />
        ))}
      </span>
      <span className="live-danger-label">{label}</span>
      {source && <span className="live-source-tag">{source}</span>}
    </div>
  );
}
