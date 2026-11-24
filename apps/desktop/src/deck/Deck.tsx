import { useLayoutEffect, useRef, useState, type CSSProperties } from "react";
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
import { allyHeroCards, cardCopy, enemyHeroCards, ownDraftSlot, ownHeroCard, type HeroCard } from "../live/secondMonitorCards";
import { useRosterMemory } from "../live/rosterMemory";
import { CENTER_ROOMY_CARD_MIN_H, centeredLiveLayout, ownFieldCardMode } from "../live/secondMonitorLayout";
import type { SecondMonitorSide } from "../live/secondMonitorModel";
import "./deck.css";

function useFieldBox<T extends HTMLElement>() {
  const ref = useRef<T>(null);
  const [box, setBox] = useState<{ width: number; height: number } | null>(null);
  useLayoutEffect(() => {
    const node = ref.current;
    if (!node) return;
    const measure = () => {
      const rect = node.getBoundingClientRect();
      const next = { width: Math.floor(rect.width), height: Math.floor(rect.height) };
      setBox(previous => previous && previous.width === next.width && previous.height === next.height ? previous : next);
    };
    measure();
    const observer = typeof ResizeObserver === "undefined" ? null : new ResizeObserver(measure);
    observer?.observe(node);
    window.addEventListener("resize", measure);
    return () => { observer?.disconnect(); window.removeEventListener("resize", measure); };
  }, []);
  return { ref, box };
}

function phaseLabel(locale: Locale, phase: RuntimeStatus["session"]["phase"] | null): string {
  const th = locale !== "en";
  switch (phase) {
    case "draft": return th ? "ดราฟต์" : "Draft";
    case "pregame": return th ? "ก่อนเริ่มเกม" : "Pregame";
    case "live": return th ? "กำลังเล่น" : "In match";
    default: return "";
  }
}

function CenteredLiveField({ locale, status, snapshot, hostLost, steamStatus, state, own, loadout, stale, draftBar, roster, ownTeam, ownSlot, allyCards, enemyCards }: {
  locale: Locale; status: RuntimeStatus | null; snapshot: OwnSnapshot | null; hostLost: boolean; steamStatus: SteamStatus | null;
  state: DeckStateView; own: ReturnType<typeof deriveOwnHeroViewModel>; loadout: ReturnType<typeof deriveLoadoutViewModel>;
  stale: boolean; draftBar: ReturnType<typeof useDraftBar>; roster: ReturnType<typeof useRosterMemory>; ownTeam: "radiant" | "dire" | null;
  ownSlot: number | null; allyCards: HeroCard[]; enemyCards: HeroCard[];
}) {
  const text = deckCopy(locale);
  const cardsText = cardCopy(locale === "en" ? "en" : "th");
  const field = useFieldBox<HTMLDivElement>();
  const layout = field.box ? centeredLiveLayout(field.box) : null;
  const slot = ownDraftSlot(roster, hostLost ? null : status?.session.session_epoch ?? null, ownTeam, own.heroId) ?? ownSlot;
  const ownModel = ownHeroCard({ heroId: own.heroId, hpPercent: own.hpPercent, manaPercent: own.manaPercent,
    tiles: loadout.abilities.stale ? null : loadout.abilities.tiles, slot });
  const ourCards = orderBySlot([ownModel, ...allyCards]);
  const radiantCards = ownTeam === "radiant" ? ourCards : enemyCards;
  const direCards = ownTeam === "dire" ? ourCards : enemyCards;
  const phase = hostLost ? null : status?.session.phase ?? null;
  const draft = phase === "draft";
  const phaseName = phaseLabel(locale, phase);
  const sideWidth = layout?.mode === "field" ? layout.card.w : 0;
  const wideStats = sideWidth >= 260;
  const roomyField = layout?.mode === "field" && sideWidth >= 320 && layout.card.h >= CENTER_ROOMY_CARD_MIN_H;
  const ownFieldMode = layout?.mode === "field" ? ownFieldCardMode(layout.card) : undefined;
  const regions = layout?.mode === "field" ? layout : null;
  const position = (rect: { x: number; y: number; w: number; h: number }): CSSProperties =>
    ({ position: "absolute", left: rect.x, top: rect.y, width: rect.w, height: rect.h });

  if (!layout || layout.mode === "too_small") return (
    <main className="deck-root deck-field-root" data-layout="too_small" lang={locale}>
      {/* The measured host stays mounted so the first measurement (and every resize) can leave this state. */}
      <div className="deck-centered-host deck-field-too-small" ref={field.ref} role="status">
        <h2>{locale === "en" ? "Window too small" : "หน้าต่างเล็กเกินไป"}</h2>
        <p>{locale === "en" ? "Enlarge the window to show all ten cards and the minimap." : "ขยายหน้าต่างเพื่อแสดงการ์ดทั้งสิบใบและแผนที่ครบถ้วน"}</p>
      </div>
    </main>
  );

  return (
    <main className="deck-root deck-field-root" lang={locale} data-state={state.state} data-draft={draft || undefined} data-draft-column="true">
      <div className="deck-centered-host" ref={field.ref} data-layout="field">
        <section className="deck-field-region deck-field-team" style={position(regions!.radiant)} data-region="radiant" data-testid="center-team-radiant">
          <CompactHeroCards cards={radiantCards} locale={locale} label={cardsText.radiant} fieldCard wideStats={wideStats} roomyField={roomyField}
            renderOwn={card => <OwnHeroCard key={card.key} locale={locale} own={own} loadout={loadout} slot={card.slot}
              stale={stale || hostLost} ageMs={hostLost ? null : own.ageMs} hasLatestResult={false} headingId="center-own-heading"
              headingLabel={{ title: text.ownHero, latestResult: text.latestResult }} fieldCard wideStats={wideStats} ownFieldMode={ownFieldMode} youLabel={cardsText.you} />} />
        </section>
        <section className="deck-field-region deck-field-map" style={position(regions!.map)} data-region="map" data-testid="center-map-region">
          <div className="deck-field-phase" data-testid="gsi-phase-strip"><span>{phaseName}</span></div>
          {draft ? <div className="deck-field-draft-row" data-testid="center-draft-region">
            <DraftBarStrip frame={draftBar} locale={locale} />
            <DraftDeckPanel visible />
          </div> : <MinimapPanel locale={locale} layout="field" steamStatus={steamStatus} />}
        </section>
        <section className="deck-field-region deck-field-team" style={position(regions!.dire)} data-region="dire" data-testid="center-team-dire">
          <CompactHeroCards cards={direCards} locale={locale} label={cardsText.dire} fieldCard wideStats={wideStats} roomyField={roomyField} />
        </section>
      </div>
    </main>
  );
}

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
  inMatch,
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
  inMatch: boolean;
  /** Runs the same gsi_setup flow as the connection card; shown as a button on the items/abilities hint. */
  gsiSetup?: (replace: boolean) => Promise<string | null>;
  /** Second-monitor mode (UI-016). Present only while a match runs and a monitor side is set. */
  secondMonitor?: { side: Exclude<SecondMonitorSide, "off">; onChange: (side: SecondMonitorSide) => void };
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
  const cardTeam = ownTeam ?? (ownSlot === null ? null : ownSlot < 5 ? "radiant" : "dire");
  const allyCards = orderBySlot(allyHeroCards({ memory: roster, epoch: sessionEpoch, ownTeam: cardTeam, ownHeroId: own.heroId }));
  const enemyCards = orderBySlot(enemyHeroCards(roster, sessionEpoch, cardTeam));
  const cardText = cardCopy(locale === "en" ? "en" : "th");
  const enemyCaption = [cardText.source.draft, cardText.source.enemyNone].join(" · ");
  const allyCaption = [cardText.source.draft, cardText.source.peerPending].join(" · ");

  if (inMatch && secondMonitor) return (
    <SecondMonitorDeck locale={locale} side={secondMonitor.side} snapshot={snapshot} hostLost={hostLost}
      sessionEpoch={hostLost ? null : status?.session.session_epoch ?? null} draftBar={draftBar} steamStatus={steamStatus} />
  );
  if (inMatch) return (
    <CenteredLiveField locale={locale} status={status} snapshot={snapshot} hostLost={hostLost} steamStatus={steamStatus}
      state={state} own={own} loadout={loadout} stale={stale} draftBar={draftBar} roster={roster} ownTeam={cardTeam}
      ownSlot={ownSlot} allyCards={allyCards} enemyCards={enemyCards} />
  );

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

  return legacy;
}
