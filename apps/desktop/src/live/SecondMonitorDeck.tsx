import { useLayoutEffect, useRef, useState, type CSSProperties, type ReactNode } from "react";
import type { Locale } from "../copy";
import type { OwnSnapshot } from "../types";
import type { DraftBarFrame } from "../deck/draftBarModel";
import { DraftBarStrip } from "../deck/DraftBarStrip";
import { deriveOwnHeroViewModel, ownHeroLifeLabel } from "../deck/ownHeroModel";
import { deriveLoadoutViewModel } from "../deck/loadoutModel";
import { OwnLoadout } from "../deck/OwnLoadout";
import { deckCopy } from "../deck/copy";
import { MinimapPanel } from "./MinimapPanel";
import { DangerMeter } from "./DangerMeter";
import { formatNullable, riskTag } from "./state";
import type { RiskView } from "./types";
import type { SteamStatus } from "../steam/model";
import { SecondMonitorToggle } from "./SecondMonitorToggle";
import {
  allyRows,
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

function TeamCard({ rows, matched, locale }: { rows: RosterRow[]; matched: boolean; locale: "th" | "en" }) {
  const text = secondMonitorCopy(locale);
  return (
    <section className="sm-card sm-team" aria-labelledby="sm-team-heading">
      <h2 id="sm-team-heading">{text.allies}
        <span className="sm-card-tags"><SourceTag>{text.source.draft}</SourceTag> <SourceTag pending>{text.source.lifePending}</SourceTag></span></h2>
      <ul className="sm-rows">
        {rows.map(row => (
          <li key={row.key} data-testid={`sm-ally-${row.key}`}>
            <span className="sm-slot">{row.key}</span>
            <b className={row.heroName ? "" : "is-unknown"}>{row.heroName ?? "—"}</b>
            <span className="sm-life">{text.lifeUnknown}</span>
          </li>
        ))}
      </ul>
      <p className="sm-note">{matched ? text.source.peerPending : text.unmatchedOwn}</p>
    </section>
  );
}

function OwnStrip({ snapshot, hostLost, locale, gsiSetup }: {
  snapshot: OwnSnapshot | null; hostLost: boolean; locale: "th" | "en"; gsiSetup?: (replace: boolean) => Promise<string | null>;
}) {
  const text = secondMonitorCopy(locale);
  const deckText = deckCopy(locale);
  const own = deriveOwnHeroViewModel(snapshot);
  const loadout = deriveLoadoutViewModel(snapshot, hostLost);
  const n = (value: number | null) => value === null ? "—" : String(value);
  const hpWidth = own.hpPercent === null ? undefined : `${Math.max(0, Math.min(100, own.hpPercent))}%`;
  const manaWidth = own.manaPercent === null ? undefined : `${Math.max(0, Math.min(100, own.manaPercent))}%`;
  const stats = [
    [deckText.statLabels[0], n(own.stats.gold)],
    [deckText.statLabels[1], [own.stats.kills, own.stats.deaths, own.stats.assists].map(n).join(" / ")],
    [deckText.statLabels[2], [own.stats.last_hits, own.stats.denies].map(n).join(" / ")],
    [deckText.statLabels[3], [own.stats.gpm, own.stats.xpm].map(n).join(" / ")],
  ];
  return (
    <section className="sm-card sm-own" aria-label={deckText.ownHero}>
      <div className="sm-own-line">
        <b data-testid="sm-own-name">{own.heroName?.name ?? "—"}</b>
        <span className="sm-muted">{text.own} · {text.level} {n(own.level)}</span>
        <span className="sm-life" data-testid="sm-own-life">{ownHeroLifeLabel(own, locale)}</span>
        <SourceTag>{text.source.gsi}</SourceTag>
      </div>
      <div className="sm-own-bars">
        <span className="sm-bar sm-bar-hp" role="img" aria-label={`HP ${n(own.hp)} / ${n(own.maxHp)}`}>{hpWidth && <span style={{ width: hpWidth }} />}</span>
        <span className="sm-bar sm-bar-mana" role="img" aria-label={`${deckText.mana} ${n(own.manaPercent)}`}>{manaWidth && <span style={{ width: manaWidth }} />}</span>
      </div>
      <p className="sm-note mono" data-testid="sm-own-vitals">HP {n(own.hp)} / {n(own.maxHp)} · {deckText.mana} {own.manaPercent === null ? "—" : `${own.manaPercent}%`}</p>
      <details className="sm-own-more">
        <summary>{text.more}</summary>
        <div className="sm-own-stats">
          {stats.map(([label, value]) => <div key={label}><span>{label}</span><b className="mono">{value}</b></div>)}
        </div>
        <OwnLoadout model={loadout} locale={locale} gsiSetup={gsiSetup} />
      </details>
    </section>
  );
}

export function SecondMonitorDeck({
  locale, side, onSideChange, snapshot, hostLost, sessionEpoch, draftBar, steamStatus, gsiSetup,
  nextObjective = null, renderAlerts, topSlot, events, announcer, legacy,
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
  const allies = allyRows(roster, sessionEpoch, ownTeam, own.heroId);
  const matched = allies.some(row => row.heroName !== null);

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
      <TeamCard rows={allies} matched={matched} locale={lang} />
      <OwnStrip snapshot={hostLost ? null : snapshot} hostLost={hostLost} locale={lang} gsiSetup={gsiSetup} />
      {announcer}
    </>
  );
  return (
    <div ref={ref} className="sm-host sm-pinned" data-layout={pinned ? "pinned" : "measuring"} data-map-edge={pinned?.mapEdge ?? (side === "right" ? "left" : "right")}
      data-columns={pinned?.columns ?? 2} style={style} lang={locale}>
      <div className="sm-inner">
        <MinimapPanel locale={locale} layout="pinned" steamStatus={steamStatus} renderAlerts={({ risk, captureLive }) => (
          <>
            <QuickStrip risk={risk} rows={enemies} ownTeam={ownTeam} next={nextObjective} locale={lang} />
            <div className="sm-alerts">{renderAlerts({ risk, captureLive })}</div>
          </>
        )} />
      </div>
      <div className="sm-outer">
        {pinned?.columns === 1 ? (
          <div className="sm-col">{columnA}{events}</div>
        ) : (
          <>
            <div className="sm-col">{columnA}</div>
            <div className="sm-col">{events}</div>
          </>
        )}
      </div>
    </div>
  );
}
