import { useLayoutEffect, useRef, useState, type CSSProperties, type ReactNode } from "react";
import type { Locale } from "../copy";
import type { OwnSnapshot } from "../types";
import type { DraftBarFrame } from "../deck/draftBarModel";
import { deriveOwnHeroViewModel } from "../deck/ownHeroModel";
import { deriveLoadoutViewModel } from "../deck/loadoutModel";
import { MinimapPanel } from "./MinimapPanel";
import type { SteamStatus } from "../steam/model";
import {
  allyHeroCards,
  enemyHeroCards,
  ownDraftSlot,
  ownHeroCard,
  type HeroCard,
  type PeerInput,
  type TopbarInput,
} from "./secondMonitorCards";
import { type SecondMonitorSide } from "./secondMonitorModel";
import { MapCountdowns } from "./MapCountdowns";
import { MapRoshanStatus } from "./MapRoshanStatus";
import { PinnedPanels } from "./PinnedPanels";
import { DraftBoard } from "./DraftBoard";
import { outerPanelBox } from "./pinnedPanelsModel";
import { draftBoardRows, type DraftStatsInput, type PlayerNamesInput } from "./draftBoardModel";
import { CompactHeroCards, OwnHeroCard } from "./HeroCard";
import { CARD_ROOMY_ROW_MIN_H, ownFieldCardMode, pinnedDraftFits, secondMonitorLayout, type PlacedLayout, type Rect } from "./secondMonitorLayout";
import { useLiveMatchSamples } from "./useLiveMatchSamples";
import { useScoreSamples } from "./useScoreSamples";
import { useRosterMemory } from "./rosterMemory";
import { useObjectiveReadModel } from "./useObjectiveReadModel";
import { cardCopy } from "./secondMonitorCards";
import "./secondMonitor.css";

function useBox<T extends HTMLElement>() {
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

function placeStyle(rect: Rect, edge: "left" | "right"): CSSProperties {
  return { position: "absolute", top: rect.y, width: rect.w, height: rect.h, [edge]: rect.x };
}

function freshGameClock(snapshot: OwnSnapshot | null, hostLost: boolean): number | null {
  const clock = snapshot?.map?.clock_time;
  return !hostLost && snapshot?.availability === "fresh" && (snapshot.source.age_ms ?? Infinity) <= 2_000
    && clock?.availability === "fresh" && Number.isSafeInteger(clock.value) ? clock.value : null;
}

function TeamRow({ cards, side, locale, wideStats, roomyField, rect, edge, renderOwn }: {
  cards: HeroCard[]; side: "ally" | "enemy"; locale: Locale; wideStats: boolean; roomyField: boolean;
  rect: Rect; edge: "left" | "right";
  renderOwn?: (card: HeroCard) => ReactNode;
}) {
  const labels = cardCopy(locale === "en" ? "en" : "th");
  const title = side === "ally" ? labels.ourTeam : labels.enemyTeam;
  return (
    <section className={`sm-region sm-card-row sm-card-row-${side}`} style={placeStyle(rect, edge)} data-region={side === "ally" ? "ours" : "enemy"}
      data-testid={`sm-row-${side}`} aria-label={title}>
      <CompactHeroCards cards={cards} locale={locale} label={title} fieldCard fieldDirection="row" wideStats={wideStats} roomyField={roomyField} renderOwn={renderOwn} />
    </section>
  );
}

/** Left/right second-monitor mode: the map panel stays on the inner edge between two fixed five-card rows. */
export function SecondMonitorDeck({
  locale, side, snapshot, hostLost, sessionEpoch, draftBar, steamStatus, gamePhase,
  topbar = null, peer = null, draftStats = null, playerNames = null, showPlayerNames = false,
}: {
  locale: Locale;
  side: Exclude<SecondMonitorSide, "off">;
  snapshot: OwnSnapshot | null;
  hostLost: boolean;
  sessionEpoch: number | null;
  draftBar: DraftBarFrame | null;
  steamStatus: SteamStatus | null;
  gamePhase?: string | null;
  topbar?: TopbarInput | null;
  peer?: PeerInput | null;
  draftStats?: DraftStatsInput | null;
  playerNames?: PlayerNamesInput | null;
  showPlayerNames?: boolean;
}) {
  const lang = locale === "en" ? "en" : "th";
  const text = cardCopy(lang);
  const { ref, box } = useBox<HTMLDivElement>();
  const draftPhase = !hostLost && draftBar?.active === true;
  const layout = box ? secondMonitorLayout({ side, width: box.width, height: box.height }) : null;
  const draftTableTooSmall = layout?.mode === "stacked" && draftPhase && !pinnedDraftFits(layout.map.h);
  const useObjectiveData = layout?.mode === "stacked" && !draftPhase && !hostLost && (gamePhase === "pregame" || gamePhase === "live");
  const objectiveRead = useObjectiveReadModel(useObjectiveData);
  const roster = useRosterMemory(hostLost ? null : sessionEpoch, draftBar);
  const matchSamples = useLiveMatchSamples(snapshot, gamePhase ?? null, hostLost ? null : sessionEpoch, hostLost);
  const scoreSamples = useScoreSamples(snapshot, gamePhase ?? null, hostLost ? null : sessionEpoch, hostLost);
  const own = deriveOwnHeroViewModel(snapshot);
  const ownTeam = hostLost ? null : snapshot?.own?.team ?? null;
  const ownLoadout = deriveLoadoutViewModel(hostLost ? null : snapshot, hostLost);
  const slot = ownDraftSlot(roster, hostLost ? null : sessionEpoch, ownTeam, own.heroId);
  const cardTeam = ownTeam ?? (slot === null ? null : slot < 5 ? "radiant" : "dire");
  const ownCard = ownHeroCard({ heroId: own.heroId, hpPercent: own.hpPercent, manaPercent: own.manaPercent,
    tiles: ownLoadout.abilities.stale ? null : ownLoadout.abilities.tiles, slot });
  const allies = allyHeroCards({ memory: roster, epoch: sessionEpoch, ownTeam: cardTeam, ownHeroId: own.heroId, peer, topbar });
  const ours = orderBySlot([ownCard, ...allies]);
  const enemies = orderBySlot(enemyHeroCards(roster, sessionEpoch, cardTeam));
  const boardRows = draftBoardRows({ memory: roster, epoch: sessionEpoch, ownTeam, stats: draftStats, names: playerNames, showNames: showPlayerNames });

  if (!layout || layout.mode === "off") return <div ref={ref} className="sm-host" data-layout="measuring" lang={locale} />;
  if (layout.mode === "too_small" || draftTableTooSmall) {
    return <div ref={ref} className="sm-host sm-too-small" data-layout="too_small" lang={locale}>
      <div className="sm-too-small-box" role="status">
        <h2>{lang === "th" ? "หน้าต่างเล็กเกินไป" : "Window too small"}</h2>
        <p>{lang === "th" ? "ขยายหน้าต่างเพื่อแสดงการ์ดทั้งสิบใบและแผนที่ครบถ้วน" : "Enlarge the window to show all ten cards and the minimap."}</p>
      </div>
    </div>;
  }

  const placed = layout as PlacedLayout;
  const outerBox = outerPanelBox(placed);
  const gameClockSeconds = freshGameClock(snapshot, hostLost);
  const ownStale = hostLost || own.snapshotAvailability === "stale";
  const wideStats = placed.card.w >= 260;
  const roomyField = placed.card.w >= 320 && placed.card.h >= CARD_ROOMY_ROW_MIN_H;
  const ownFieldMode = ownFieldCardMode(placed.card);
  const ownLabel = text.you;
  return (
    <div ref={ref} className="sm-host sm-pinned" data-layout={placed.mode} data-map-edge={placed.mapEdge} lang={locale}>
      <TeamRow cards={enemies} side="enemy" locale={locale} wideStats={wideStats} roomyField={roomyField} rect={placed.enemy} edge={placed.mapEdge} />
      <div className="sm-map-region" style={placeStyle(placed.map, placed.mapEdge)} data-region="map" data-testid="sm-map-region">
        <MinimapPanel locale={locale} layout="pinned" mapEdge={placed.mapEdge}
          steamStatus={steamStatus} snapshot={snapshot} gamePhase={gamePhase ?? null}
          scoreSamples={scoreSamples}
          mapOverlay={!draftPhase && (gamePhase === "pregame" || gamePhase === "live")
            ? <><MapCountdowns schedule={objectiveRead?.model.schedule ?? null} receivedAtMs={objectiveRead?.receivedAtMs ?? null} locale={lang} gameClockSeconds={gameClockSeconds} />
              <MapRoshanStatus observation={null} locale={lang} /></>
            : null}
          stageOverride={draftPhase ? <DraftBoard rows={boardRows} locale={lang}
            namesAvailable={showPlayerNames && playerNames !== null} statsAvailable={draftStats !== null} /> : undefined} />
        {gamePhase === "live" && !draftPhase && !hostLost && outerBox.w > 0 && outerBox.h > 0 && (
          <div className="sm-panel-area" style={{ left: outerBox.x, top: outerBox.y - placed.map.y, width: outerBox.w, height: outerBox.h }}>
            <PinnedPanels box={{ x: 0, y: 0, w: outerBox.w, h: outerBox.h }} samples={matchSamples}
              scoreSamples={scoreSamples} schedule={objectiveRead?.model.schedule ?? null}
              scheduleReceivedAtMs={objectiveRead?.receivedAtMs ?? null} mapFields={objectiveRead?.model.map ?? null}
              sourceAvailability={objectiveRead?.model.availability ?? "unknown"} gameClockSeconds={gameClockSeconds} locale={lang} />
          </div>
        )}
      </div>
      <TeamRow cards={ours} side="ally" locale={locale} wideStats={wideStats} roomyField={roomyField} rect={placed.ours} edge={placed.mapEdge}
        renderOwn={card => <OwnHeroCard key={card.key} locale={locale} own={own} loadout={ownLoadout} slot={card.slot}
          stale={ownStale} ageMs={hostLost ? null : own.ageMs} hasLatestResult={false} headingId="sm-own-heading"
          headingLabel={{ title: text.ourTeam, latestResult: "" }} fieldCard wideStats={wideStats} ownFieldMode={ownFieldMode} youLabel={ownLabel} />} />
    </div>
  );
}

/** Cards ordered by real slot ID when known; unknown slots keep their stable input order. */
export function orderBySlot(cards: HeroCard[]): HeroCard[] {
  if (cards.some(card => card.slot === null)) return cards;
  return [...cards].sort((a, b) => (a.slot ?? 0) - (b.slot ?? 0));
}
