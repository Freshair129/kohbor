import { liveCopy, type LiveLocale } from "./copy";
import { useRisk } from "./useRisk";
import { DangerMeter } from "./DangerMeter";
import { EnemySeen } from "./EnemySeen";
import "./live.css";

/** Passive overlay content: it listens to risk state and exposes no host commands. */
export function RiskStrip({ locale = "th" }: { locale?: LiveLocale }) {
  const risk = useRisk();
  return (
    <aside className="live-risk-strip" aria-label={liveCopy(locale).danger} lang={locale}>
      <DangerMeter state={risk?.state ?? null} locale={locale} compact />
      <EnemySeen risk={risk} locale={locale} compact />
    </aside>
  );
}
