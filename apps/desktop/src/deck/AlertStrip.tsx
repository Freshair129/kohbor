import { deriveAlertStrip } from "./alertModel";
import type { OwnHeroViewModel } from "./ownHeroModel";
import type { DeckState } from "./deckState";
import type { RiskView } from "../live/types";
import type { Locale } from "../copy";

export function AlertStrip({ own, state, risk, captureLive, locale }: {
  own: OwnHeroViewModel; state: DeckState; risk: RiskView | null; captureLive: boolean; locale: Locale;
}) {
  const view = deriveAlertStrip(own, state === "live" && !captureLive ? "novision" : state, risk, locale);
  return (
    <div className="deck-alert-strip" aria-label={locale === "th" ? "สัญญาณแจ้งเตือน" : "Alert signals"}>
      {view.stateText && <span className="deck-alert-state">{view.stateText}</span>}
      {view.health && <span className={`deck-alert-chip${view.health.critical ? " is-critical" : ""}`}>! {view.health.label}</span>}
      {view.vision.map(label => <span className="deck-alert-chip is-experimental" key={label}>{label}</span>)}
      {!view.stateText && !view.health && view.vision.length === 0 && <span className="muted">—</span>}
    </div>
  );
}
