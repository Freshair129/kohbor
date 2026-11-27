import { useEffect, useLayoutEffect, useRef, useState, type CSSProperties } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import type { Locale } from "../copy";
import { MinimapPanel } from "../live/MinimapPanel";
import type { OwnSnapshot, RuntimeStatus } from "../types";
import { deckCopy } from "./copy";
import type { DeckStateView } from "./deckState";
import { deriveOwnHeroViewModel } from "./ownHeroModel";
import { DraftBarStrip, useDraftBar } from "./DraftBarStrip";
import { DraftDeckPanel } from "../draft/DraftDeckPanel";
import { deriveLoadoutViewModel } from "./loadoutModel";
import type { SteamStatus } from "../steam/model";
import { SecondMonitorDeck, orderBySlot } from "../live/SecondMonitorDeck";
import { OwnHeroCard, CompactHeroCards, PostGameFinalCards } from "../live/HeroCard";
import { allyHeroCards, cardCopy, enemyHeroCards, ownDraftSlot, ownHeroCard, type HeroCard } from "../live/secondMonitorCards";
import { useRosterMemory } from "../live/rosterMemory";
import { CENTER_ROOMY_CARD_MIN_H, centeredLiveLayout, ownFieldCardMode } from "../live/secondMonitorLayout";
import type { SecondMonitorSide } from "../live/secondMonitorModel";
import { overviewClient } from "../overview/overviewClient";
import { parseMatchHistory, type MatchHistory } from "../overview/overviewModel";
import { pregameDashboardModel } from "../live/pregameDashboardModel";
import { frozenRosterSlots, postGameSummaryModel, type PostGameBenchmarkInput, type PostGameReplayInput, type ReplayWatcherView } from "../live/postGameSummaryModel";
import { loadMatchRecap, type MatchRecapSnapshot } from "../packs/recapModel";
import { PregameDashboard } from "../live/PregameDashboard";
import { PostGameSummary } from "../live/PostGameSummary";
import { ScoreboardHeader } from "../live/ScoreboardHeader";
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

type CachedBenchmarksReply = {
  enabled: boolean;
  state: string;
  benchmarks: PostGameBenchmarkInput["cached"] | null;
};

function CenteredLiveField({ locale, status, snapshot, hostLost, steamStatus, matchEnded, state, own, loadout, stale, draftBar, roster, ownTeam, ownSlot, allyCards, enemyCards, onOpenReplay }: {
  locale: Locale; status: RuntimeStatus | null; snapshot: OwnSnapshot | null; hostLost: boolean; steamStatus: SteamStatus | null;
  matchEnded: boolean;
  state: DeckStateView; own: ReturnType<typeof deriveOwnHeroViewModel>; loadout: ReturnType<typeof deriveLoadoutViewModel>;
  stale: boolean; draftBar: ReturnType<typeof useDraftBar>; roster: ReturnType<typeof useRosterMemory>; ownTeam: "radiant" | "dire" | null;
  ownSlot: number | null; allyCards: HeroCard[]; enemyCards: HeroCard[]; onOpenReplay: (matchKey?: string) => void;
}) {
  const text = deckCopy(locale);
  const cardsText = cardCopy(locale === "en" ? "en" : "th");
  const [history, setHistory] = useState<MatchHistory | null>(null);
  const [recap, setRecap] = useState<MatchRecapSnapshot | null>(null);
  const [postGameReplay, setPostGameReplay] = useState<PostGameReplayInput | null>(null);
  const [watcher, setWatcher] = useState<ReplayWatcherView>({ state: "unknown", folder: "unknown", lastScanAtMs: null });
  const [postGame, setPostGame] = useState<{ epoch: number | null; dismissed: boolean } | null>(null);
  const [lastLiveSnapshot, setLastLiveSnapshot] = useState<{ epoch: number; snapshot: OwnSnapshot } | null>(null);
  const [frozenRosterMemory, setFrozenRosterMemory] = useState<{ epoch: number; slots: (string | null)[] } | null>(null);
  const [postGameBenchmarks, setPostGameBenchmarks] = useState<PostGameBenchmarkInput | null>(null);
  const [pickPending, setPickPending] = useState(false);
  const [pickFailed, setPickFailed] = useState(false);
  const refreshReplayRef = useRef<() => void>(() => {});
  const phase = hostLost ? null : status?.session.phase ?? null;
  const statusEpoch = hostLost ? null : status?.session.session_epoch ?? null;
  const sessionEpoch = phase === "none" ? null : statusEpoch;
  useEffect(() => {
    let active = true;
    void overviewClient.matchHistory(0).then(raw => {
      if (active) setHistory(parseMatchHistory(raw));
    }).catch(() => { if (active) setHistory(null); });
    return () => { active = false; };
  }, [phase, sessionEpoch]);
  useEffect(() => {
    if (phase === "draft" || phase === "pregame") {
      setLastLiveSnapshot(null);
    } else if (phase === "live" && sessionEpoch !== null && snapshot?.availability === "fresh"
      && snapshot.session_epoch === sessionEpoch) {
      setLastLiveSnapshot({ epoch: sessionEpoch, snapshot });
    }
  }, [phase, sessionEpoch, snapshot]);
  useEffect(() => {
    if (postGame?.dismissed) {
      setFrozenRosterMemory(null);
      return;
    }
    if (sessionEpoch !== null && statusEpoch === sessionEpoch && roster.epoch === sessionEpoch) {
      setFrozenRosterMemory(previous => previous?.epoch === roster.epoch
        && previous.slots.every((heroId, slot) => heroId === roster.slots[slot])
        ? previous
        : { epoch: roster.epoch!, slots: roster.slots.slice() });
    } else if (phase === "draft" || phase === "pregame" || phase === "live") {
      setFrozenRosterMemory(null);
    }
  }, [phase, sessionEpoch, statusEpoch, roster, postGame?.dismissed]);
  useEffect(() => {
    if (phase === "draft" || phase === "pregame" || phase === "live") {
      setPostGame(null);
      setPostGameReplay(null);
      setWatcher({ state: "unknown", folder: "unknown", lastScanAtMs: null });
    } else if ((phase === "post_game" || matchEnded) && !hostLost && statusEpoch !== null) {
      setPostGame(previous => previous?.epoch === statusEpoch ? previous : { epoch: statusEpoch, dismissed: false });
    }
  }, [hostLost, matchEnded, phase, statusEpoch]);
  useEffect(() => {
    let active = true;
    if (!postGame || postGame.dismissed || postGame.epoch === null) {
      setRecap(null);
      return () => { active = false; };
    }
    void loadMatchRecap(command => invoke<unknown>(command)).then(value => { if (active) setRecap(value); });
    return () => { active = false; };
  }, [postGame?.dismissed, postGame?.epoch]);
  useEffect(() => {
    let active = true;
    let refreshGeneration = 0;
    if (hostLost || (phase !== "none" && phase !== "post_game" && !(phase === "live" && matchEnded))) {
      refreshReplayRef.current = () => {};
      return () => { active = false; };
    }
    const refresh = async () => {
      const generation = ++refreshGeneration;
      const replayArgs = phase === "post_game" && statusEpoch !== null ? { sessionEpoch: statusEpoch } : { sessionEpoch: null };
      const [replayResult, historyResult] = await Promise.allSettled([
        invoke<PostGameReplayInput>("post_game_replay_summary", replayArgs),
        overviewClient.matchHistory(0),
      ]);
      if (!active || generation !== refreshGeneration) return;
      if (replayResult.status === "fulfilled") {
        const reply = replayResult.value;
        setPostGameReplay(reply);
        setWatcher(reply.watcher ?? { state: "unknown", folder: "unknown", lastScanAtMs: null });
        const ended = reply.lastFinishedMatch;
        const canReopen = phase === "none"
          || (ended != null && statusEpoch === ended.epoch && (phase === "post_game" || matchEnded));
        if (ended && canReopen) {
          setPostGame(previous => previous?.epoch === ended.epoch ? previous : { epoch: ended.epoch, dismissed: false });
        }
      } else {
        setPostGameReplay(null);
        setWatcher({ state: "unknown", folder: "unknown", lastScanAtMs: null });
      }
      if (historyResult.status === "fulfilled") setHistory(parseMatchHistory(historyResult.value));
    };
    refreshReplayRef.current = () => { void refresh(); };
    void refresh();
    return () => { active = false; refreshGeneration += 1; refreshReplayRef.current = () => {}; };
  }, [hostLost, matchEnded, phase, statusEpoch]);
  useEffect(() => {
    if (!postGame || postGame.dismissed || hostLost) return;
    let active = true;
    let unlisten: (() => void) | undefined;
    void listen("replay-library://state-changed", () => { refreshReplayRef.current(); }).then(stop => {
      if (!active) stop();
      else unlisten = stop;
    }).catch(() => {});
    return () => { active = false; unlisten?.(); };
  }, [hostLost, postGame?.dismissed, postGame?.epoch]);
  const field = useFieldBox<HTMLDivElement>();
  const layout = field.box ? centeredLiveLayout(field.box) : null;
  const slot = ownDraftSlot(roster, sessionEpoch, ownTeam, own.heroId) ?? ownSlot;
  const ownModel = ownHeroCard({ heroId: own.heroId, hpPercent: own.hpPercent, manaPercent: own.manaPercent,
    tiles: loadout.abilities.stale ? null : loadout.abilities.tiles, slot });
  const ourCards = orderBySlot([ownModel, ...allyCards]);
  const radiantCards = ownTeam === "radiant" ? ourCards : enemyCards;
  const direCards = ownTeam === "dire" ? ourCards : enemyCards;
  const draft = phase === "draft";
  const replayEpochMatches = postGame !== null && postGame.epoch !== null
    && postGameReplay?.lastFinishedMatch?.epoch === postGame.epoch;
  const endedPhaseMatches = postGame !== null && postGame.epoch !== null && (
    (phase === "post_game" && statusEpoch === postGame.epoch)
      || (phase === "live" && matchEnded && statusEpoch === postGame.epoch)
      || (phase === "none" && replayEpochMatches)
  );
  const showPostGame = postGame !== null && !postGame.dismissed && endedPhaseMatches;
  const showReopenPostGame = postGame !== null && postGame.dismissed && replayEpochMatches && phase === "none";
  const activeReplay = showPostGame && replayEpochMatches ? postGameReplay : null;
  const dashboard = pregameDashboardModel(history, new Date(), locale === "en" ? "en" : "th");
  const recapMatch = postGame?.epoch === null || postGame?.epoch === undefined
    ? null
    : recap?.matches.find(match => match.match_id === `local_match_${postGame.epoch}`) ?? null;
  const summarySnapshot = postGame?.epoch !== null && postGame?.epoch !== undefined
    ? lastLiveSnapshot?.epoch === postGame.epoch ? lastLiveSnapshot.snapshot
      : snapshot?.session_epoch === postGame.epoch ? snapshot : null
    : null;
  const historyEntry = activeReplay?.matchKey
    ? history?.entries.find(entry => entry.source === "kohbor" && entry.matchKey === activeReplay.matchKey) ?? null
    : null;
  const freshMap = summarySnapshot?.availability === "fresh" ? summarySnapshot.map : undefined;
  const endedRosterMemory = postGame?.epoch !== null && postGame?.epoch !== undefined
    ? frozenRosterMemory?.epoch === postGame.epoch ? frozenRosterMemory
      : roster.epoch === postGame.epoch ? roster : null
    : null;
  const endedRoster = postGame?.epoch !== null && postGame?.epoch !== undefined
    ? frozenRosterSlots(endedRosterMemory, postGame.epoch)
    : [];
  const snapshotOwnTeam = summarySnapshot?.availability === "fresh" ? summarySnapshot.own?.team ?? null : null;
  const endedHeroId = activeReplay?.lastFinishedMatch?.endValues.heroId ?? null;
  const endedOwnSlot = postGame?.epoch !== null && postGame?.epoch !== undefined
    ? ownDraftSlot(endedRosterMemory ?? { epoch: null, slots: Array(10).fill(null) }, postGame.epoch, snapshotOwnTeam, endedHeroId)
    : null;
  const endedOwnTeam = snapshotOwnTeam ?? (endedOwnSlot === null ? null : endedOwnSlot < 5 ? "radiant" : "dire");
  const summaryInput = {
    entry: historyEntry,
    snapshot: summarySnapshot,
    scoreboard: {
      radiant: freshMap?.radiant_score.availability === "fresh" ? freshMap.radiant_score.value : null,
      dire: freshMap?.dire_score.availability === "fresh" ? freshMap.dire_score.value : null,
      clockSeconds: freshMap?.clock_time.availability === "fresh" ? freshMap.clock_time.value : null,
    },
    recap: recapMatch ? { events: recapMatch.playedEvents, droppedCount: recapMatch.droppedCount, skippedCount: recap?.skipped ?? 0 } : null,
    derivedStats: [],
    replay: activeReplay,
    watcher: activeReplay?.watcher ?? watcher,
    roster: endedRoster,
    rosterEpoch: postGame?.epoch ?? null,
    ownSlot: endedOwnSlot === null || endedOwnTeam === null ? null : { team: endedOwnTeam, slot: endedOwnSlot },
  };
  const baseSummary = postGameSummaryModel(summaryInput, locale === "en" ? "en" : "th");
  useEffect(() => {
    const heroId = baseSummary.heroHeader.heroId;
    if (!showPostGame || !heroId) {
      setPostGameBenchmarks(null);
      return;
    }
    let current = true;
    setPostGameBenchmarks(null);
    void invoke<CachedBenchmarksReply>("hero_benchmarks_state_get", { heroKey: heroId }).then(reply => {
      if (current) setPostGameBenchmarks({
        heroId,
        enabled: reply.enabled,
        cached: reply.enabled && reply.state === "ready" ? reply.benchmarks : null,
      });
    }).catch(() => { if (current) setPostGameBenchmarks(null); });
    return () => { current = false; };
  }, [showPostGame, postGame?.epoch, baseSummary.heroHeader.heroId]);
  const summary = postGameSummaryModel({ ...summaryInput, benchmarks: postGameBenchmarks }, locale === "en" ? "en" : "th");
  const pickReplay = async () => {
    if (pickPending) return;
    setPickPending(true);
    setPickFailed(false);
    try {
      const sourcePath = await invoke<string | null>("replay_library_pick_file");
      if (sourcePath) {
        await invoke("replay_library_parse", { idOrPath: sourcePath });
        refreshReplayRef.current();
      }
    } catch {
      setPickFailed(true);
    } finally {
      setPickPending(false);
    }
  };
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
          {showPostGame ? <PostGameFinalCards cards={summary.frozenCards.filter(card => card.team === "radiant")} locale={locale} label={cardsText.radiant} />
            : <CompactHeroCards cards={radiantCards} locale={locale} label={cardsText.radiant} fieldCard wideStats={wideStats} roomyField={roomyField}
            renderOwn={card => <OwnHeroCard key={card.key} locale={locale} own={own} loadout={loadout} slot={card.slot}
              stale={stale || hostLost} ageMs={hostLost ? null : own.ageMs} hasLatestResult={false} headingId="center-own-heading"
              headingLabel={{ title: text.ownHero, latestResult: text.latestResult }} fieldCard wideStats={wideStats} ownFieldMode={ownFieldMode} youLabel={cardsText.you} />} />}
        </section>
        <section className="deck-field-region deck-field-map" style={position(regions!.map)} data-region="map" data-testid="center-map-region">
          {draft ? <>
            <div className="deck-field-draft-scoreboard"><ScoreboardHeader snapshot={snapshot} phase={phase} locale={locale === "en" ? "en" : "th"} /></div>
            <div className="deck-field-draft-row" data-testid="center-draft-region">
              <DraftBarStrip frame={draftBar} locale={locale} />
              <DraftDeckPanel visible />
            </div>
          </> : showPostGame ? <PostGameSummary summary={summary} locale={locale === "en" ? "en" : "th"}
            onClose={() => { setFrozenRosterMemory(null); setPostGameBenchmarks(null); setPostGame(value => value ? { ...value, dismissed: true } : null); }} onOpenReplay={onOpenReplay}
            onPickReplay={() => { void pickReplay(); }} pickPending={pickPending} pickFailed={pickFailed} />
            : showReopenPostGame ? <button type="button" className="live-postgame-reopen"
              onClick={() => setPostGame(value => value ? { ...value, dismissed: false } : null)}>
              {locale === "en" ? "Last match" : "แมตช์ล่าสุด"}
            </button>
            : phase === "none" && !hostLost ? <PregameDashboard model={dashboard} locale={locale === "en" ? "en" : "th"}
              mapState={steamStatus?.minimap_state ?? "unsupported"} />
              : <MinimapPanel locale={locale} layout="field" steamStatus={steamStatus} snapshot={snapshot} gamePhase={phase} />}
        </section>
        <section className="deck-field-region deck-field-team" style={position(regions!.dire)} data-region="dire" data-testid="center-team-dire">
          {showPostGame ? <PostGameFinalCards cards={summary.frozenCards.filter(card => card.team === "dire")} locale={locale} label={cardsText.dire} />
            : <CompactHeroCards cards={direCards} locale={locale} label={cardsText.dire} fieldCard wideStats={wideStats} roomyField={roomyField} />}
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
  hostLost,
  steamStatus,
  inMatch,
  matchEnded = false,
  secondMonitor,
  onOpenReplay = () => {},
}: {
  locale: Locale;
  status: RuntimeStatus | null;
  snapshot: OwnSnapshot | null;
  state: DeckStateView;
  hostLost: boolean;
  steamStatus: SteamStatus | null;
  inMatch: boolean;
  matchEnded?: boolean;
  onOpenReplay?: (matchKey?: string) => void;
  /** Second-monitor mode (UI-016). Present only while a match runs and a monitor side is set. */
  secondMonitor?: { side: Exclude<SecondMonitorSide, "off"> };
}) {
  const own = deriveOwnHeroViewModel(snapshot);
  const loadout = deriveLoadoutViewModel(snapshot, hostLost);
  const draftEpoch = !hostLost && status?.session.phase === "draft" ? status.session.session_epoch : null;
  const draftBar = useDraftBar(draftEpoch);
  const stale = state.state === "stale" || own.snapshotAvailability === "stale";
  // Replay-style cards (DEC-REPLAY-INTEGRATION item 10; full structure on every card, owner 2026-11-24): identities only
  // from same-session draft memory (UI-012), shared with the second-monitor deck; allies and enemies have no other
  // admitted source here, so their other values stay unknown.
  const phase = hostLost ? null : status?.session.phase ?? null;
  const sessionEpoch = phase === "none" || hostLost ? null : status?.session.session_epoch ?? null;
  const roster = useRosterMemory(sessionEpoch, draftBar);
  const ownTeam = hostLost ? null : snapshot?.own?.team ?? null;
  const ownSlot = ownDraftSlot(roster, sessionEpoch, ownTeam, own.heroId);
  const cardTeam = ownTeam ?? (ownSlot === null ? null : ownSlot < 5 ? "radiant" : "dire");
  const allyCards = orderBySlot(allyHeroCards({ memory: roster, epoch: sessionEpoch, ownTeam: cardTeam, ownHeroId: own.heroId }));
  const enemyCards = orderBySlot(enemyHeroCards(roster, sessionEpoch, cardTeam));

  if (inMatch && secondMonitor) return (
    <SecondMonitorDeck locale={locale} side={secondMonitor.side} snapshot={snapshot} hostLost={hostLost}
      sessionEpoch={hostLost ? null : status?.session.session_epoch ?? null} draftBar={draftBar} steamStatus={steamStatus}
      gamePhase={hostLost ? null : status?.session.phase ?? null} />
  );
  return <CenteredLiveField locale={locale} status={status} snapshot={snapshot} hostLost={hostLost} steamStatus={steamStatus} matchEnded={matchEnded}
    state={state} own={own} loadout={loadout} stale={stale} draftBar={draftBar} roster={roster} ownTeam={cardTeam}
    ownSlot={ownSlot} allyCards={allyCards} enemyCards={enemyCards} onOpenReplay={onOpenReplay} />;
}
