import type { ReactNode } from "react";
import { copy, type Locale } from "../copy";
import { MinimapPanel } from "../live/MinimapPanel";
import { useAudioHistory } from "../useAudioHistory";
import { hostNowFromAnchor } from "../hostClock";
import type { PlaybackView } from "../runtime";
import type { AudioMuteState } from "../AudioPanel";
import type { OwnSnapshot, RuntimeStatus } from "../types";
import { deckCopy } from "./copy";
import type { DeckStateView } from "./deckState";
import { deriveOwnHeroViewModel } from "./ownHeroModel";
import { AlertStrip } from "./AlertStrip";
import { EventFeed } from "./EventFeed";
import { AnnouncerStatus } from "./AnnouncerStatus";
import { DraftBarStrip, useDraftBar } from "./DraftBarStrip";
import { DraftDeckPanel } from "../draft/DraftDeckPanel";
import { deriveLoadoutViewModel } from "./loadoutModel";
import type { SteamStatus } from "../steam/model";
import { SecondMonitorDeck, orderBySlot } from "../live/SecondMonitorDeck";
import { OwnHeroCard, CompactHeroCards } from "../live/HeroCard";
import { allyHeroCards, cardCopy, enemyHeroCards, ownDraftSlot } from "../live/secondMonitorCards";
import { useRosterMemory } from "../live/rosterMemory";
import type { SecondMonitorSide } from "../live/secondMonitorModel";
import "./deck.css";

export function Deck({
  locale,
  status,
  snapshot,
  state,
  mute,
  playback,
  hostLost,
  steamStatus,
  gsiSetup,
  secondMonitor,
}: {
  locale: Locale;
  status: RuntimeStatus | null;
  snapshot: OwnSnapshot | null;
  state: DeckStateView;
  mute: AudioMuteState;
  playback: PlaybackView;
  hostLost: boolean;
  steamStatus: SteamStatus | null;
  /** Runs the same gsi_setup flow as the connection card; shown as a button on the items/abilities hint. */
  gsiSetup?: (replace: boolean) => Promise<string | null>;
  /** Second-monitor mode (UI-016). Present only when the side is set and the window is large enough. */
  secondMonitor?: { side: Exclude<SecondMonitorSide, "off">; onChange: (side: SecondMonitorSide) => void; status?: ReactNode; setupNotice?: ReactNode };
}) {
  const text = deckCopy(locale);
  const own = deriveOwnHeroViewModel(snapshot);
  const loadout = deriveLoadoutViewModel(snapshot, hostLost);
  // A retained payload's age is not a current age once the host stops answering.
  const displayAgeMs = hostLost ? null : own.ageMs;
  const history = useAudioHistory(hostLost);
  const draftEpoch = !hostLost && status?.session.phase === "draft" ? status.session.session_epoch : null;
  const draftBar = useDraftBar(draftEpoch);
  const historyState = hostLost ? "failed" : history.state;
  const hostNowMs = hostLost ? null : hostNowFromAnchor(playback.clockAnchor, performance.now());
  const stale = state.state === "stale" || own.snapshotAvailability === "stale";
  const hasLatestResult = state.state === "debrief" && own.hasOwnSnapshot;
  // Replay-style cards (DEC-REPLAY-INTEGRATION item 10; full structure on every card, owner 2026-11-24): identities only
  // from same-session draft memory (UI-012), shared with the second-monitor deck; allies and enemies have no other
  // admitted source here, so their other values stay unknown.
  const sessionEpoch = hostLost ? null : status?.session.session_epoch ?? null;
  const roster = useRosterMemory(sessionEpoch, draftBar);
  const ownTeam = hostLost ? null : snapshot?.own?.team ?? null;
  const ownSlot = ownDraftSlot(roster, sessionEpoch, ownTeam, own.heroId);
  const allyCards = orderBySlot(allyHeroCards({ memory: roster, epoch: sessionEpoch, ownTeam, ownHeroId: own.heroId }));
  const enemyCards = orderBySlot(enemyHeroCards(roster, sessionEpoch, ownTeam));
  const cardText = cardCopy(locale === "en" ? "en" : "th");
  const enemyCaption = [cardText.source.draft, cardText.source.enemyNone].join(" · ");
  const allyCaption = [cardText.source.draft, cardText.source.peerPending].join(" · ");

  const legacy = (
    <main className="deck-root" lang={locale} data-state={state.state} data-draft={draftBar?.active || undefined} data-draft-column="true">
      <div className="deck-draft-row">
        <DraftBarStrip frame={draftBar} locale={locale} />
        <DraftDeckPanel visible />
      </div>
      <section className="deck-column deck-own-column" aria-label={text.ownHero}>
        <OwnHeroCard locale={locale} own={own} loadout={loadout} slot={ownSlot} stale={stale} ageMs={displayAgeMs}
          hasLatestResult={hasLatestResult} headingId="deck-own-heading" headingLabel={{ title: text.ownHero, latestResult: text.latestResult }}
          gsiSetup={gsiSetup} />

        <section className="deck-ally-panel" aria-labelledby="deck-allies-heading">
          <header className="deck-card-heading">
            <h2 id="deck-allies-heading">{text.allies}</h2>
          </header>
          <p className="deck-enemy-caption deck-ally-caption" title={allyCaption}>{allyCaption}</p>
          <CompactHeroCards cards={allyCards} locale={locale} label={text.allies} />
        </section>
      </section>

      <section className="deck-column deck-map-column" aria-label={copy(locale).shell.tabs.live}>
        {state.state === "standby" && (
          <p className="deck-standby-note" role="status">{locale === "th" ? state.reason.th : state.reason.en}</p>
        )}
        <MinimapPanel locale={locale} layout="deck" steamStatus={steamStatus} renderAlerts={({ risk, captureLive }) => (
          <AlertStrip own={{ ...own, ageMs: displayAgeMs }} state={state.state} risk={risk} captureLive={captureLive} locale={locale} />
        )} />
      </section>

      <section className="deck-column deck-right-column" aria-label={text.enemies}>
        <section className="deck-enemy-panel" aria-labelledby="deck-enemies-heading">
          <header className="deck-card-heading">
            <h2 id="deck-enemies-heading">{text.enemies}</h2>
          </header>
          <p className="deck-enemy-caption" title={enemyCaption}>{enemyCaption}</p>
          <CompactHeroCards cards={enemyCards} locale={locale} label={text.enemies} />
        </section>

        <EventFeed locale={locale} history={history.history} state={historyState} hostNowMs={hostNowMs} />
        <AnnouncerStatus locale={locale} status={status} mute={mute} history={history.history}
          historyState={historyState} hostNowMs={hostNowMs} playback={playback} hostLost={hostLost} />
      </section>
    </main>
  );

  if (!secondMonitor) return legacy;
  return (
    <SecondMonitorDeck locale={locale} side={secondMonitor.side} onSideChange={secondMonitor.onChange}
      snapshot={snapshot} hostLost={hostLost} sessionEpoch={hostLost ? null : status?.session.session_epoch ?? null}
      draftBar={draftBar} steamStatus={steamStatus} gsiSetup={gsiSetup} status={secondMonitor.status} setupNotice={secondMonitor.setupNotice}
      renderAlerts={({ risk, captureLive }) => (
        <AlertStrip own={{ ...own, ageMs: displayAgeMs }} state={state.state} risk={risk} captureLive={captureLive} locale={locale} />
      )}
      events={<EventFeed locale={locale} history={history.history} state={historyState} hostNowMs={hostNowMs} />}
      announcer={<AnnouncerStatus locale={locale} status={status} mute={mute} history={history.history}
        historyState={historyState} hostNowMs={hostNowMs} playback={playback} hostLost={hostLost} />} />
  );
}
