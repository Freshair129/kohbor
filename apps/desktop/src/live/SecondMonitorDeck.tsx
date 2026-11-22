import { useLayoutEffect, useRef, useState, type CSSProperties, type ReactNode } from "react";
import type { Locale } from "../copy";
import type { OwnSnapshot } from "../types";
import type { DraftBarFrame } from "../deck/draftBarModel";
import { DraftBarStrip } from "../deck/DraftBarStrip";
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
  secondMonitorLayout,
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

function HeroCardView({ card, locale, children }: { card: HeroCard; locale: "th" | "en"; children?: ReactNode }) {
  const text = cardCopy(locale);
  const ultSource = card.skills[3].source;
  const who = card.side === "enemy" ? text.enemyMark : card.side === "own" ? text.you : text.ourTeam;
  return (
    <article className={`sm-hero sm-hero-${card.side}${card.heroName ? "" : " is-unknown"}`} data-testid={`sm-card-${card.key}`}
      aria-label={`${who}: ${card.heroName ?? "—"}`}>
      <HeroPortrait heroId={card.heroId} size="md" alt={card.heroName ?? "—"} />
      <div className="sm-hero-body">
        <div className="sm-hero-name">
          <b title={card.heroName ?? undefined}>{card.heroName ?? "—"}</b>
          {card.side === "enemy" && <span className="sm-enemy-mark">{text.enemyMark}</span>}
          {card.side === "own" && <span className="sm-muted">{text.you}</span>}
        </div>
        <div className="sm-hero-bars">
          <Bar value={card.hpPercent} kind="hp" label={text.hp} />
          <Bar value={card.manaPercent} kind="mana" label={text.mana} />
        </div>
        <SkillSquares skills={card.skills} locale={locale} />
        <div className="sm-hero-src">
          {card.nameSource === "draft" && <SourceTag>{text.source.draft}</SourceTag>}
          {(card.nameSource === "gsi" || card.vitalsSource === "gsi") && <SourceTag>{text.source.gsi}</SourceTag>}
          {card.vitalsSource === "peer" && <SourceTag>{text.source.peer}</SourceTag>}
          {card.side === "ally" && card.vitalsSource === null && <SourceTag pending>{text.source.peerPending}</SourceTag>}
          {card.side === "ally" && ultSource === "topbar" && <SourceTag>{text.source.topbar}</SourceTag>}
          {card.side === "ally" && ultSource === null && <SourceTag pending>{text.source.topbarPending}</SourceTag>}
          {card.side === "enemy" && <SourceTag pending>{text.source.enemyNone}</SourceTag>}
        </div>
        {children}
      </div>
    </article>
  );
}

function OwnCard({ card, snapshot, hostLost, locale, gsiSetup }: {
  card: HeroCard; snapshot: OwnSnapshot | null; hostLost: boolean; locale: "th" | "en"; gsiSetup?: (replace: boolean) => Promise<string | null>;
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
    <HeroCardView card={card} locale={locale}>
      <p className="sm-note" data-testid="sm-own-life">{text.level} {n(own.level)} · {ownHeroLifeLabel(own, locale)}</p>
      <details className="sm-own-more">
        <summary>{text.more}</summary>
        <div className="sm-own-stats">
          {stats.map(([label, value]) => <div key={label}><span>{label}</span><b className="mono">{value}</b></div>)}
        </div>
        <OwnLoadout model={loadout} locale={locale} gsiSetup={gsiSetup} />
      </details>
    </HeroCardView>
  );
}

function TeamSection({ title, side, cards, note, locale, children }: {
  title: string; side: "ally" | "enemy"; cards: HeroCard[]; note?: string | null; locale: "th" | "en"; children?: ReactNode;
}) {
  return (
    <section className={`sm-card sm-team sm-team-${side}`} aria-label={title}>
      <h2>{title}</h2>
      <div className="sm-hero-list">
        {children}
        {cards.map(card => <HeroCardView key={card.key} card={card} locale={locale} />)}
      </div>
      {note && <p className="sm-note">{note}</p>}
    </section>
  );
}

export function SecondMonitorDeck({
  locale, side, onSideChange, snapshot, hostLost, sessionEpoch, draftBar, steamStatus, gsiSetup,
  nextObjective = null, topbar = null, peer = null, objectiveSchedule = null, draftStats = null, playerNames = null,
  showPlayerNames = false, renderAlerts, topSlot, events, announcer, legacy,
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
  topSlot?: ReactNode;
  events: ReactNode;
  announcer: ReactNode;
  /** Today's centred deck, used when the box is too small. */
  legacy: ReactNode;
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
  const enemyCards = enemyHeroCards(roster, sessionEpoch, ownTeam);
  const cardText = cardCopy(lang);
  const draftPhase = !hostLost && draftBar?.active === true;
  const boardRows = draftBoardRows({ memory: roster, epoch: sessionEpoch, ownTeam, stats: draftStats, names: playerNames, showNames: showPlayerNames });
  const soonest = nextObjective ? null
    : countdownLabels(objectiveSchedule?.schedule ?? null, objectiveSchedule?.receivedAtMs ?? null, Date.now())
      .sort((a, b) => a.remaining - b.remaining)[0] ?? null;
  const next: NextObjective | null = nextObjective
    ?? (soonest ? { label: objectiveShortName(soonest.objective, lang), secondsUntil: soonest.remaining } : null);

  if (layout && layout.mode === "fallback") {
    return (
      <div ref={ref} className="sm-host sm-fallback" data-layout="fallback" lang={locale}>
        <div className="sm-fallback-bar">
          <SecondMonitorToggle side={side} onChange={onSideChange} locale={lang} />
          <span className="sm-note" role="status">{text.fallbackNote}</span>
        </div>
        {legacy}
      </div>
    );
  }

  const pinned = layout?.mode === "pinned" ? layout : null;
  const style = pinned ? { "--sm-map": `${pinned.mapPx}px`, "--sm-band": `${pinned.bandPx}px` } as CSSProperties : undefined;
  const columnA = (
    <>
      {topSlot}
      <div className="sm-head">
        <SecondMonitorToggle side={side} onChange={onSideChange} locale={lang} />
      </div>
      {draftBar?.active && <DraftBarStrip frame={draftBar} locale={locale} />}
      <TeamSection title={cardText.ourTeam} side="ally" cards={allies} locale={lang} note={matched ? null : text.unmatchedOwn}>
        <OwnCard card={ownCard} snapshot={hostLost ? null : snapshot} hostLost={hostLost} locale={lang} gsiSetup={gsiSetup} />
      </TeamSection>
      {announcer}
    </>
  );
  const enemySection = (
    <TeamSection title={cardText.enemyTeam} side="enemy" cards={enemyCards} locale={lang} note={ownTeam === null ? text.noTeam : null} />
  );
  return (
    <div ref={ref} className="sm-host sm-pinned" data-layout={pinned ? "pinned" : "measuring"} data-map-edge={pinned?.mapEdge ?? (side === "right" ? "left" : "right")}
      data-columns={pinned?.columns ?? 2} style={style} lang={locale}>
      <div className="sm-inner">
        <MinimapPanel locale={locale} layout="pinned" steamStatus={steamStatus}
          mapOverlay={<MapCountdowns schedule={objectiveSchedule?.schedule ?? null} receivedAtMs={objectiveSchedule?.receivedAtMs ?? null} locale={lang} />}
          stageOverride={draftPhase ? <DraftBoard rows={boardRows} locale={lang}
            namesAvailable={showPlayerNames && playerNames !== null} statsAvailable={draftStats !== null} /> : undefined}
          renderAlerts={({ risk, captureLive }) => (
          <>
            <QuickStrip risk={risk} rows={enemies} ownTeam={ownTeam} next={next} locale={lang} />
            <div className="sm-alerts">{renderAlerts({ risk, captureLive })}</div>
          </>
        )} />
      </div>
      <div className="sm-outer">
        {pinned?.columns === 1 ? (
          <div className="sm-col">{columnA}{enemySection}{events}</div>
        ) : (
          <>
            <div className="sm-col">{columnA}</div>
            <div className="sm-col">{enemySection}{events}</div>
          </>
        )}
      </div>
    </div>
  );
}
