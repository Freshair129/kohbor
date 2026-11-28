import { copy, type Locale } from "../copy";
import type { AnalyticsReplayDetailsRoute } from "../analytics/analyticsModel";
import { MatchViewPanel } from "./MatchViewPanel";

export function AnalyticsReplayDetailPanel({ locale, detail, onBack }: {
  locale: Locale;
  detail: AnalyticsReplayDetailsRoute;
  onBack: () => void;
}) {
  return (
    <div className="shell-screen shell-replay-detail" style={{ display: "grid", gridTemplateRows: "auto minmax(0, 1fr)", gap: 8, minWidth: 0, minHeight: 0, overflow: "hidden" }}>
      <header className="shell-replay-detail__header">
        <button type="button" onClick={onBack}>{copy(locale).analyticsPlayer.back}</button>
      </header>
      <MatchViewPanel locale={locale} initialBundle={detail.bundle} initialMatchId={detail.matchId}
        initialTime={detail.initialTime} />
    </div>
  );
}
