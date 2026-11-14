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
      {view.chips.map(chip => <span key={`${chip.kind}:${chip.label}`} title={chip.label}
        className={chip.kind === "state" ? "deck-alert-state" : `deck-alert-chip${chip.kind === "vision" ? " is-experimental" : chip.kind === "health" && chip.critical ? " is-critical" : ""}`}>
        {chip.kind === "health" ? "! " : ""}{chip.label}
      </span>)}
      {view.chips.length === 0 && <span className="muted">—</span>}
    </div>
  );
}
