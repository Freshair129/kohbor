import { liveCopy, type LiveLocale } from "./copy";
import { formatNullable } from "./state";
import type { RiskView } from "./types";

export function EnemySeen({ risk, locale, compact = false }: { risk: RiskView | null; locale: LiveLocale; compact?: boolean }) {
  const text = liveCopy(locale);
  const count = formatNullable(risk?.state?.visible ?? null);
  const event = risk?.lastVisionEvent ?? null;

  return (
    <div className={`live-enemy-seen${compact ? " live-enemy-compact" : ""}`}>
      <span className="live-enemy-count">{text.enemyCount(count)}</span>
      <span className="live-experimental">{text.experimental}</span>
      {event && <span className="live-vision-event mono">{text.visionEvent[event]}</span>}
    </div>
  );
}
