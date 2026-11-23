import { useLayoutEffect, useRef, useState, type CSSProperties, type ReactNode } from "react";
import type { Locale } from "../copy";
import type { OwnSnapshot } from "../types";
import type { DraftBarFrame } from "../deck/draftBarModel";
import { deriveOwnHeroViewModel, ownHeroLifeLabel } from "../deck/ownHeroModel";
import { deriveLoadoutViewModel } from "../deck/loadoutModel";
import { OwnLoadout } from "../deck/OwnLoadout";
import { HeroPortrait } from "../deck/HeroPortrait";
import { formatCooldownValue } from "../deck/cooldownModel";
import { deckCopy } from "../deck/copy";
import { MinimapPanel } from "./MinimapPanel";
import { DangerMeter } from "./DangerMeter";
import { formatNullable, riskTag } from "./state";
import type { RiskView } from "./types";
import type { SteamStatus } from "../steam/model";
import { SecondMonitorToggle } from "./SecondMonitorToggle";
import {
  allyHeroCards,
  cardCopy,
  enemyHeroCards,
  ownDraftSlot,
  ownHeroCard,
  type HeroCard,
  type PeerInput,
  type TopbarInput,
} from "./secondMonitorCards";
import {
  createDraftRosterMemory,
  enemyRows,
  formatCountdown,
  missingFromMap,
  rememberDraftFrame,
  secondMonitorCopy,
  type DraftRosterMemory,
  type NextObjective,
  type RosterRow,
  type SecondMonitorSide,
  type Team,
} from "./secondMonitorModel";
import { MapCountdowns } from "./MapCountdowns";
import { DraftBoard } from "./DraftBoard";
import { countdownLabels, objectiveShortName, type ObjectiveSchedule } from "./objectiveCountdownModel";
import { draftBoardRows, type DraftStatsInput, type PlayerNamesInput } from "./draftBoardModel";
import { cardMetrics, secondMonitorLayout, type CardKind, type Rect } from "./secondMonitorLayout";
import "./secondMonitor.css";

// Draft identities live in memory for the match only (UI-012); a new session epoch clears them.
let rosterMemory: DraftRosterMemory = createDraftRosterMemory();

function useRosterMemory(epoch: number | null, frame: DraftBarFrame | null): DraftRosterMemory {
  rosterMemory = rememberDraftFrame(rosterMemory, epoch, frame);
  return rosterMemory;
}

function useBox<T extends HTMLElement>() {
  const ref = useRef<T>(null);
  const [box, setBox] = useState<{ width: number; height: number } | null>(null);
  useLayoutEffect(() => {
    const node = ref.current;
    if (!node) return;
    const measure = () => {
      const rect = node.getBoundingClientRect();
      setBox(previous => previous && previous.width === Math.floor(rect.width) && previous.height === Math.floor(rect.height)
        ? previous : { width: Math.floor(rect.width), height: Math.floor(rect.height) });
    };
    measure();
    const observer = typeof ResizeObserver === "undefined" ? null : new ResizeObserver(measure);
    observer?.observe(node);
    window.addEventListener("resize", measure);
    return () => { observer?.disconnect(); window.removeEventListener("resize", measure); };
  }, []);
  return { ref, box };
}

function SourceTag({ children, pending = false }: { children: ReactNode; pending?: boolean }) {
  return <span className={`sm-src${pending ? " is-pending" : ""}`}>{children}</span>;
}

function QuickStrip({ risk, rows, ownTeam, next, locale }: {
  risk: RiskView | null; rows: RosterRow[]; ownTeam: Team | null; next: NextObjective | null; locale: "th" | "en";
}) {
  const text = secondMonitorCopy(locale);
  const state = risk?.state ?? null;
  const missing = missingFromMap(state);
  const tag = state ? riskTag(state) : "—";
  return (
    <section className="sm-band" aria-label={locale === "th" ? "แถบด่วน" : "Quick strip"}>
      <div className="sm-cell sm-cell-danger">
        <span className="sm-lbl">{text.danger} <SourceTag>{tag === "—" ? text.source.risk : tag}</SourceTag></span>
        <DangerMeter state={state} locale={locale} compact />
      </div>
      <div className="sm-cell sm-cell-missing">
        <span className="sm-lbl">{text.missing} <SourceTag>{text.source.derived}</SourceTag></span>
        <span className="sm-big mono" data-testid="sm-missing">{missing === null ? "—" : String(missing)}
          {missing !== null && text.missingUnit && <small> {text.missingUnit}</small>}</span>
        <span className="sm-sub">{text.visible(formatNullable(state?.visible ?? null))}</span>
      </div>
      <div className="sm-cell sm-cell-enemies">
        <span className="sm-lbl">{text.enemies} <SourceTag>{text.source.draft}</SourceTag> <SourceTag pending>{text.source.lifePending}</SourceTag></span>
        <div className="sm-tokens">
          {rows.map(row => (
            <div className={`sm-token${row.heroName ? "" : " is-unknown"}`} key={row.key} data-testid={`sm-enemy-${row.key}`}>
              <b title={row.heroName ?? undefined}>{row.heroName ?? "—"}</b>
              <span>{text.lifeUnknown}</span>
            </div>
          ))}
        </div>
        {ownTeam === null && <span className="sm-sub">{text.noTeam}</span>}
      </div>
      <div className={`sm-cell sm-cell-objective${next ? "" : " is-unknown"}`}>
        <span className="sm-lbl">{next?.label ?? text.nextObjective} <SourceTag pending={!next}>{text.source.objective}</SourceTag></span>
        <span className="sm-big mono" data-testid="sm-objective">{formatCountdown(next?.secondsUntil ?? null)}</span>
        {!next && <span className="sm-sub">{text.objectiveNoSource}</span>}
      </div>
    </section>
  );
}

function Bar({ value, kind, label }: { value: number | null; kind: "hp" | "mana"; label: string }) {
  const shown = value === null ? "—" : `${Math.round(value)}%`;
  return (
    <span className={`sm-bar sm-bar-${kind}${value === null ? " is-unknown" : ""}`} role="img" aria-label={`${label} ${shown}`}>
      {value !== null && <span style={{ width: `${value}%` }} />}
      <em className="mono">{label} {shown}</em>
    </span>
  );
}

function SkillSquares({ skills, locale }: { skills: HeroCard["skills"]; locale: "th" | "en" }) {
  const text = cardCopy(locale);
  return (
    <ol className="sm-skills">
      {skills.map((skill, index) => {
        const label = skill.ultimate ? text.ult : text.skill(index + 1);
        const value = skill.seconds !== null ? formatCooldownValue(skill.seconds) : skill.state === "unknown" ? "—" : "";
        const spoken = `${label}: ${text.state[skill.state]}${skill.seconds !== null ? ` ${value}s` : ""}`;
        return (
          <li key={index} className={`sm-skill${skill.ultimate ? " is-ult" : ""}`} data-state={skill.state}
            data-source={skill.source ?? "none"} aria-label={spoken} title={spoken}>
            <span className="mono">{value}</span>
          </li>
        );
      })}
    </ol>
  );
}

function cardStyle(kind: CardKind, size: { w: number; h: number }): CSSProperties {
  const m = cardMetrics(kind, size);
  return { "--pt": `${m.portrait}px`, "--nf": `${m.name}px`, "--bh": `${m.bar}px`, "--sq": `${m.square}px`, height: `${size.h}px` } as CSSProperties;
}

/** Compact card (owner request 2026-11-23): portrait, full name, HP, mana, four skill squares. Never grows: details open in a popover. */
function HeroCardView({ card, locale, kind, size, inlineSource, children }: {
  card: HeroCard; locale: "th" | "en"; kind: CardKind; size: { w: number; h: number }; inlineSource?: string | null; children?: ReactNode;
}) {
  const text = cardCopy(locale);
  const who = card.side === "enemy" ? text.enemyMark : card.side === "own" ? text.you : text.ourTeam;
  return (
    <article className={`sm-hero sm-hero-${kind} sm-hero-${card.side}${card.heroName ? "" : " is-unknown"}`} data-testid={`sm-card-${card.key}`}
      data-slot={card.slot ?? undefined} style={cardStyle(kind, size)} aria-label={`${who}: ${card.heroName ?? "—"}`}>
      <HeroPortrait heroId={card.heroId} size="md" alt={card.heroName ?? "—"} />
      <div className="sm-hero-body">
        <div className="sm-hero-name">
          <b title={card.heroName ?? undefined}>{card.heroName ?? "—"}</b>
          {card.side === "own" && <span className="sm-muted">{text.you}</span>}
          {inlineSource && <SourceTag>{inlineSource}</SourceTag>}
          {children}
        </div>
        <div className="sm-hero-bars">
          <Bar value={card.hpPercent} kind="hp" label={text.hp} />
          <Bar value={card.manaPercent} kind="mana" label={text.mana} />
        </div>
        <SkillSquares skills={card.skills} locale={locale} />
      </div>
    </article>
  );
}

/** Level, alive/respawn, items, gold and KDA of the own hero, in a popover so the card never changes size. */
function OwnDetails({ snapshot, hostLost, locale, gsiSetup }: {
  snapshot: OwnSnapshot | null; hostLost: boolean; locale: "th" | "en"; gsiSetup?: (replace: boolean) => Promise<string | null>;
}) {
  const text = secondMonitorCopy(locale);
  const deckText = deckCopy(locale);
  const own = deriveOwnHeroViewModel(snapshot);
  const loadout = deriveLoadoutViewModel(snapshot, hostLost);
  const n = (value: number | null) => value === null ? "—" : String(value);
  const stats = [
    [deckText.statLabels[0], n(own.stats.gold)],
    [deckText.statLabels[1], [own.stats.kills, own.stats.deaths, own.stats.assists].map(n).join(" / ")],
    [deckText.statLabels[2], [own.stats.last_hits, own.stats.denies].map(n).join(" / ")],
    [deckText.statLabels[3], [own.stats.gpm, own.stats.xpm].map(n).join(" / ")],
  ];
  return (
    <details className="sm-own-more">
      <summary aria-label={text.more} title={text.more}>⋯</summary>
      <div className="sm-own-pop" role="group" aria-label={text.more}>
        <p className="sm-note" data-testid="sm-own-life">{text.level} {n(own.level)} · {ownHeroLifeLabel(own, locale)} <SourceTag>GSI</SourceTag></p>
        <div className="sm-own-stats">
          {stats.map(([label, value]) => <div key={label}><span>{label}</span><b className="mono">{value}</b></div>)}
        </div>
        <OwnLoadout model={loadout} locale={locale} gsiSetup={gsiSetup} />
      </div>
    </details>
  );
}

function teamSources(side: "ally" | "enemy", cards: HeroCard[], locale: "th" | "en"): string {
  const text = cardCopy(locale);
  if (side === "enemy") return [text.source.draft, text.source.enemyNone].join(" · ");
  const ult = cards.some(card => card.side === "ally" && card.skills[3].source === "topbar") ? text.source.topbar : text.source.topbarPending;
  return [text.source.draft, text.source.gsi + " (" + text.you + ")", text.source.peerPending, ult].join(" · ");
}

function TeamGroup({ title, side, cards, kind, size, locale, ownExtras, note }: {
  title: string; side: "ally" | "enemy"; cards: HeroCard[]; kind: CardKind; size: { w: number; h: number }; locale: "th" | "en";
  ownExtras?: ReactNode; note?: string | null;
}) {
  const text = cardCopy(locale);
  const sources = teamSources(side, cards, locale);
  const label = [note, sources].filter(Boolean).join(" · ");
  return (
    <section className={`sm-group sm-group-${kind} sm-group-${side}`} aria-label={title} data-testid={`sm-group-${side}`}>
      <h2><span>{title}</span><span className={side === "enemy" ? "sm-enemy-mark" : "sm-ally-mark"}>{side === "enemy" ? text.enemyMark : text.ourTeam}</span>
        <small className="sm-group-src" title={label}>{label}</small></h2>
      <div className="sm-group-cards">
        {cards.map(card => (
          <HeroCardView key={card.key} card={card} locale={locale} kind={kind} size={size}
            inlineSource={card.side === "own" ? text.source.gsi : card.vitalsSource === "peer" ? text.source.peer : null}>
            {card.side === "own" ? ownExtras : null}
          </HeroCardView>
        ))}
      </div>
    </section>
  );
}

/** Cards ordered by real slot ID when known (SlotRoster / draft slot); unknown slots keep their given order. */
export function orderBySlot(cards: HeroCard[]): HeroCard[] {
  if (cards.some(card => card.slot === null)) return cards;
  return [...cards].sort((a, b) => (a.slot ?? 0) - (b.slot ?? 0));
}

function placeStyle(rect: Rect, edge: "left" | "right"): CSSProperties {
  return { position: "absolute", top: rect.y, width: rect.w, height: rect.h, [edge === "left" ? "left" : "right"]: rect.x };
}

export function SecondMonitorDeck({
  locale, side, onSideChange, snapshot, hostLost, sessionEpoch, draftBar, steamStatus, gsiSetup,
  nextObjective = null, topbar = null, peer = null, objectiveSchedule = null, draftStats = null, playerNames = null,
  showPlayerNames = false, renderAlerts, status, setupNotice, events, announcer,
}: {
  locale: Locale;
  side: Exclude<SecondMonitorSide, "off">;
  onSideChange: (side: SecondMonitorSide) => void;
  snapshot: OwnSnapshot | null;
  hostLost: boolean;
  sessionEpoch: number | null;
  draftBar: DraftBarFrame | null;
  steamStatus: SteamStatus | null;
  gsiSetup?: (replace: boolean) => Promise<string | null>;
  /** No source on this branch (ALERT-008 supplies it when merged); null renders "—". */
  nextObjective?: NextObjective | null;
  /** `topbar.ally_ult_state` from the top-bar read (proposed DEC-TOPBAR-CV). No consumer on this branch: unknown. */
  topbar?: TopbarInput | null;
  /** Consented teammate sharing (PEER-003, PEER-010; M6). Not on this branch: unknown. */
  peer?: PeerInput | null;
  /** Host read-model `objective://schedule` from the ALERT-008 scheduler, with its arrival time. Not produced on this branch. */
  objectiveSchedule?: { schedule: ObjectiveSchedule; receivedAtMs: number } | null;
  /** Hero statistics projection per draft slot (DEC-PICK-STATS). Not produced on this branch. */
  draftStats?: DraftStatsInput | null;
  /** Draft 5c player names (memory-only, deck-only). Not implemented in the product yet. */
  playerNames?: PlayerNamesInput | null;
  /** The visible "show player names" setting; names are dropped when it is off. */
  showPlayerNames?: boolean;
  renderAlerts: (input: { risk: RiskView | null; captureLive: boolean }) => ReactNode;
  /** Status chips and the start button (StartDotaBar). */
  status?: ReactNode;
  /** GSI setup card; it renders nothing unless setup needs attention, and then floats over the outer area. */
  setupNotice?: ReactNode;
  events: ReactNode;
  announcer: ReactNode;
}) {
  const lang = locale === "en" ? "en" : "th";
  const text = secondMonitorCopy(lang);
  const { ref, box } = useBox<HTMLDivElement>();
  const layout = box ? secondMonitorLayout({ side, width: box.width, height: box.height }) : null;
  const roster = useRosterMemory(hostLost ? null : sessionEpoch, draftBar);
  const ownTeam = hostLost ? null : snapshot?.own?.team ?? null;
  const own = deriveOwnHeroViewModel(snapshot);
  const enemies = enemyRows(roster, sessionEpoch, ownTeam);
  const allies = allyHeroCards({ memory: roster, epoch: sessionEpoch, ownTeam, ownHeroId: own.heroId, peer, topbar });
  const matched = allies.some(card => card.slot !== null);
  const ownLoadout = deriveLoadoutViewModel(hostLost ? null : snapshot, hostLost);
  const ownCard = ownHeroCard({ heroId: own.heroId, hpPercent: own.hpPercent, manaPercent: own.manaPercent,
    tiles: ownLoadout.abilities.stale ? null : ownLoadout.abilities.tiles, slot: ownDraftSlot(roster, sessionEpoch, ownTeam, own.heroId) });
  const ourCards = orderBySlot([ownCard, ...allies]);
  const enemyCards = orderBySlot(enemyHeroCards(roster, sessionEpoch, ownTeam));
  const cardText = cardCopy(lang);
  const draftPhase = !hostLost && draftBar?.active === true;
  const boardRows = draftBoardRows({ memory: roster, epoch: sessionEpoch, ownTeam, stats: draftStats, names: playerNames, showNames: showPlayerNames });
  const soonest = nextObjective ? null
    : countdownLabels(objectiveSchedule?.schedule ?? null, objectiveSchedule?.receivedAtMs ?? null, Date.now())
      .sort((a, b) => a.remaining - b.remaining)[0] ?? null;
  const next: NextObjective | null = nextObjective
    ?? (soonest ? { label: objectiveShortName(soonest.objective, lang), secondsUntil: soonest.remaining } : null);

  if (!layout || layout.mode === "off") return <div ref={ref} className="sm-host" data-layout="measuring" lang={locale} />;
  if (layout.mode === "too_small") {
    return (
      <div ref={ref} className="sm-host sm-too-small" data-layout="too_small" lang={locale}>
        <div className="sm-too-small-box" role="status">
          <h2>{text.tooSmallTitle}</h2>
          <p>{text.tooSmallBody}</p>
          <p className="sm-muted">{text.tooSmallHint}</p>
          <SecondMonitorToggle side={side} onChange={onSideChange} locale={lang} />
        </div>
      </div>
    );
  }

  const edge = layout.mapEdge;
  const inner: Rect = { x: layout.map.x, y: layout.map.y, w: layout.map.w, h: layout.map.h + layout.band.h };
  const style = { "--sm-map": `${layout.map.w}px`, "--sm-band": `${layout.band.h}px` } as CSSProperties;
  return (
    <div ref={ref} className="sm-host sm-pinned" data-layout={layout.mode} data-plan={layout.plan} data-map-edge={edge}
      data-band={layout.band.h < 120 ? "compact" : undefined}
      data-map-narrow={layout.map.w < 480 ? "" : undefined} style={style} lang={locale}>
      <div className="sm-inner" style={placeStyle(inner, edge)} data-region="map">
        <MinimapPanel locale={locale} layout="pinned" steamStatus={steamStatus}
          mapOverlay={<MapCountdowns schedule={objectiveSchedule?.schedule ?? null} receivedAtMs={objectiveSchedule?.receivedAtMs ?? null} locale={lang} />}
          stageOverride={draftPhase ? <DraftBoard rows={boardRows} locale={lang}
            namesAvailable={showPlayerNames && playerNames !== null} statsAvailable={draftStats !== null} /> : undefined}
          stageAlerts={renderAlerts}
          renderAlerts={({ risk }) => <QuickStrip risk={risk} rows={enemies} ownTeam={ownTeam} next={next} locale={lang} />} />
      </div>
      <div className="sm-status" style={placeStyle(layout.status, edge)} data-region="status">
        {status}
        <SecondMonitorToggle side={side} onChange={onSideChange} locale={lang} />
      </div>
      <div className="sm-region" style={placeStyle(layout.ours, edge)} data-region="ours">
        <TeamGroup title={cardText.ourTeam} side="ally" cards={ourCards} kind={layout.cardKind} size={layout.card} locale={lang}
          note={matched ? null : text.unmatchedOwn}
          ownExtras={<OwnDetails snapshot={hostLost ? null : snapshot} hostLost={hostLost} locale={lang} gsiSetup={gsiSetup} />} />
      </div>
      {layout.enemy && (
        <div className="sm-region" style={placeStyle(layout.enemy, edge)} data-region="enemy">
          <TeamGroup title={cardText.enemyTeam} side="enemy" cards={enemyCards} kind={layout.cardKind} size={layout.card} locale={lang}
            note={ownTeam === null ? text.noTeam : null} />
        </div>
      )}
      {layout.slot && (
        <section className="sm-slot" style={placeStyle(layout.slot, edge)} data-region="slot" aria-label={text.slotLabel}>
          <header className="sm-slot-head"><b>{text.slotTitle}</b><small>{text.slotNote}</small></header>
          {announcer}
          <div className="sm-slot-body">{events}</div>
        </section>
      )}
      <div className="sm-setup">{setupNotice}</div>
    </div>
  );
}
