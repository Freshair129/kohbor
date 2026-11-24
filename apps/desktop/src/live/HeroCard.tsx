// Replay-style hero cards for the centred Live deck (DEC-REPLAY-INTEGRATION item 10, slice R5; owner amendment
// 2026-11-24: every card has the replay card's full structure).
// Own hero: the full card filled from GSI with labelled, source-tooltipped derived stats. Allies and enemies: the same
// structure in a denser variant, built from the second-monitor card model. Unknown renders "—" or a dashed slot; no
// path traces or enemy value without an admitted source.
import { Fragment, useEffect, useState, type CSSProperties, type ReactNode } from "react";
import { seconds, type Locale } from "../copy";
import { HeroPortrait } from "../deck/HeroPortrait";
import { GsiSetupHint } from "../deck/OwnLoadout";
import { retainFreshLoadout, type FreshLoadoutCache, type LoadoutViewModel } from "../deck/loadoutModel";
import { ownHeroLifeLabel, type OwnHeroViewModel } from "../deck/ownHeroModel";
import { cardCopy, type HeroCard as CardModel } from "./secondMonitorCards";
import type { OwnFieldCardMode } from "./secondMonitorLayout";
import { dotaIcon } from "./dotaIconSet";
import { ownFullCard, teamFullCard, UNKNOWN, type FullHeroCardView, type IconSlotView, type StatCellView } from "./heroCardModel";
import liveStatsPack from "../assets/dota/6989/stats.json";
import type { StatsData } from "../stats/derivedStats";
import "./heroCard.css";

const LIVE_STATS_DATA = liveStatsPack as unknown as StatsData;

type Lang = "th" | "en";

export function heroCardCopy(locale: Lang) {
  const th = locale === "th";
  return {
    level: th ? "เลเวล" : "Level",
    kda: "K / D / A",
    lhdn: "LH / DN",
    gold: th ? "ทอง" : "Gold",
    rates: "GPM / XPM",
    hp: "HP",
    mana: th ? "มานา" : "Mana",
    abilities: th ? "สกิล" : "Abilities",
    extras: th ? "สกิลเสริม" : "Extras",
    buffs: th ? "บัฟ" : "Buffs",
    buffsNone: th ? "บัฟ: ยังไม่มีแหล่งข้อมูล" : "Buffs: no source yet",
    items: th ? "ไอเท็ม" : "Items",
    backpack: th ? "ช่องเป้" : "Backpack",
    teleport: th ? "ช่องวาร์ป" : "Teleport slot",
    neutral: th ? "ช่องไอเท็มกลาง" : "Neutral item slot",
    stats: th ? "ค่าสถานะ" : "Stats",
    statNoSource: th ? "ยังไม่มีแหล่งข้อมูล" : "no source yet",
    noData: th ? "ยังไม่มีข้อมูลจากเกม" : "No game data yet",
    oldData: th ? "ข้อมูลเก่า" : "Old data",
    age: th ? "อายุข้อมูล" : "Age",
    empty: th ? "ว่าง" : "Empty",
    unknown: th ? "ไม่ทราบ" : "Unknown",
    notLearned: th ? "ยังไม่เรียน" : "Not learned",
    ready: th ? "พร้อม" : "Ready",
    cooldown: th ? "คูลดาวน์" : "Cooldown",
    charges: th ? "ชาร์จ" : "Charges",
    respawn: th ? "เกิดใหม่ใน" : "Respawns in",
    slot: (n: number) => th ? `ช่อง ${n}` : `Slot ${n}`,
    allyNote: th ? "ชื่อ: ดราฟต์ · ค่าอื่น: ต้องแชร์จากเพื่อน" : "Name: draft · other values: need teammate sharing",
    enemyNote: th ? "ชื่อ: ดราฟต์ · ค่าอื่น: ไม่มีแหล่งข้อมูล" : "Name: draft · other values: no source",
    unnamedNote: th ? "ยังไม่ทราบฮีโร่" : "Hero not identified yet",
  };
}
type Text = ReturnType<typeof heroCardCopy>;

/** Bundled icon when one exists, else the runtime portrait from the installed game, else the neutral initials tile. */
function CardPortrait({ heroId, src, alt }: { heroId: string | null; src: string | null; alt: string }) {
  const [failed, setFailed] = useState<string | null>(null);
  if (src && failed !== src) {
    return <span className="lhc-portrait"><img src={src} alt={alt} decoding="async" draggable={false} onError={() => setFailed(src)} /></span>;
  }
  return <span className="lhc-portrait"><HeroPortrait heroId={heroId} size="md" alt={alt} /></span>;
}

function iconTitle(slot: IconSlotView, place: string, text: Text, stale: boolean): string {
  const parts = [place];
  if (slot.state === "unknown") parts.push(text.unknown);
  else if (slot.state === "empty") parts.push(text.empty);
  else {
    parts.push(`${slot.name ?? UNKNOWN}${slot.rawId ? ` (${slot.rawId})` : ""}`);
    if (slot.level !== null) parts.push(`${text.level} ${slot.level}`);
    if (slot.cast === "not_learned") parts.push(text.notLearned);
  }
  if (slot.cast === "cooldown" && slot.cooldownText) parts.push(`${text.cooldown} ${slot.cooldownText}s`);
  else if (slot.cast === "ready") parts.push(text.ready);
  if (slot.charges !== null) parts.push(`${text.charges} ${slot.charges}`);
  if (stale) parts.push(text.oldData);
  return parts.join(" · ");
}

function Icon({ slot, shape, place, text, stale }: {
  slot: IconSlotView; shape: "ab" | "it" | "bk" | "tp" | "nt"; place: string; text: Text; stale: boolean;
}) {
  const [failed, setFailed] = useState<string | null>(null);
  const image = slot.src && failed !== slot.src ? slot.src : null;
  const title = iconTitle(slot, place, text, stale);
  const classes = ["lhc-ic", `is-${shape}`, `is-${slot.state}`, `cast-${slot.cast}`, slot.ultimate ? "is-ult" : "", image ? "has-img" : "", stale ? "is-stale" : ""]
    .filter(Boolean).join(" ");
  return (
    <span className={classes} title={title} aria-label={title} role="img">
      {image && <img src={image} alt="" draggable={false} onError={() => setFailed(image)} />}
      {!image && slot.state === "filled" && <b aria-hidden="true">{slot.text}</b>}
      {slot.cooldownText && <s aria-hidden="true">{slot.cooldownText}</s>}
      {slot.charges !== null && <em aria-hidden="true">{slot.charges}</em>}
      {slot.maxLevel !== null && slot.level !== null && slot.level > 0 && (
        <i className="lhc-pips" aria-hidden="true">{Array.from({ length: Math.min(slot.level, slot.maxLevel) }, (_, i) => <span key={i} />)}</i>
      )}
    </span>
  );
}

function Bar({ kind, percent, label, value, low = false }: { kind: "hp" | "mp"; percent: number | null; label: string; value: string; low?: boolean }) {
  return (
    <span className={`lhc-bar is-${kind}${low ? " is-low" : ""}${percent === null ? " is-unknown" : ""}`} role="img" aria-label={`${label}: ${value}`}>
      {percent !== null && <i style={{ width: `${percent}%` }} />}
      <span className="lhc-bar-num"><small>{label}</small> {value}</span>
    </span>
  );
}

function statTitle(cell: StatCellView, text: Text): string {
  if (cell.derived) {
    const sources = cell.sources.length > 0 ? cell.sources.join("; ") : "unknown";
    return `${cell.label}: ${cell.value}\nSources: ${sources}\nไม่รวมผลชั่วคราว / without temporary effects.${cell.basis ? `\n${cell.basis}` : ""}`;
  }
  return cell.value === UNKNOWN && cell.group !== "rate" ? `${cell.label}: ${UNKNOWN} (${text.statNoSource})` : `${cell.label}: ${cell.value}`;
}

/** The sections every card shares, in the replay card's order. */
function CardSections({ view, text, locale, abilitiesAge, itemsAge, basisInHeader = false, roomyField = false }: {
  view: FullHeroCardView; text: Text; locale: Locale; abilitiesAge: number | null; itemsAge: number | null; basisInHeader?: boolean; roomyField?: boolean;
}) {
  const { abilities, items } = view;
  return (
    <>
      <div className="lhc-top">
        <span className="lhc-kd" title={`${text.kda}: ${view.kdaText}`}>{view.kdaText}</span>
        {!roomyField && <span title={`${text.lhdn}: ${view.lhdnText}`}>{view.lhdnText}</span>}
        {!roomyField && <span className="lhc-gold" title={`${text.gold}: ${view.goldText}`}>{view.goldText}</span>}
      </div>
      {roomyField && <div className="lhc-econ">
        <span title={`${text.lhdn}: ${view.lhdnText}`}>{view.lhdnText}</span>
        <span className="lhc-gold" title={`${text.gold}: ${view.goldText}`}>{view.goldText}</span>
      </div>}
      <div className="lhc-bars">
        <Bar kind="hp" percent={view.hpPercent} label={text.hp} value={view.hpText} low={view.hpLow} />
        <Bar kind="mp" percent={view.manaPercent} label={text.mana} value={view.manaText} />
      </div>
      <div className={`lhc-row lhc-ab${abilities.stale ? " is-stale" : ""}`} aria-label={text.abilities}
        title={abilities.stale ? `${text.oldData} · ${seconds(abilitiesAge, locale)}` : abilities.known ? undefined : `${text.abilities}: ${text.unknown}`}>
        {abilities.hero.map((slot, index) => <Icon key={slot.key} slot={slot} shape="ab" place={`${text.abilities} ${index + 1}`} text={text} stale={abilities.stale} />)}
        {abilities.extras.length > 0 && <span className="lhc-extras" aria-label={text.extras}>
          {abilities.extras.map(slot => <Icon key={slot.key} slot={slot} shape="ab" place={text.extras} text={text} stale={abilities.stale} />)}
        </span>}
        <span className="lhc-mods" aria-label={text.buffs} title={view.buffs.length === 0 ? text.buffsNone : undefined}>
          {view.buffs.map(buff => <span key={buff} className="lhc-mod">{buff}</span>)}
        </span>
      </div>
      <div className={`lhc-row lhc-it${items.stale ? " is-stale" : ""}`} aria-label={text.items}
        title={items.stale ? `${text.oldData} · ${seconds(itemsAge, locale)}` : items.known ? undefined : `${text.items}: ${text.unknown}`}>
        <span className="lhc-main">
          {items.main.map((slot, index) => <Icon key={slot.key} slot={slot} shape="it" place={text.slot(index + 1)} text={text} stale={items.stale} />)}
        </span>
        <span className="lhc-side">
          <span className="lhc-bp" aria-label={text.backpack}>
            {items.backpack.map((slot, index) => <Icon key={slot.key} slot={slot} shape="bk" place={`${text.backpack} B${index + 1}`} text={text} stale={items.stale} />)}
          </span>
          <Icon slot={items.teleport} shape="tp" place={text.teleport} text={text} stale={items.stale} />
          <Icon slot={items.neutral} shape="nt" place={text.neutral} text={text} stale={items.stale} />
        </span>
      </div>
      <div className="lhc-stat-stack">
        {view.derivedBasis && !basisInHeader && <div className="lhc-derived-basis" title={view.derivedBasis.title}>{view.derivedBasis.label}</div>}
        <dl className="lhc-stats" aria-label={text.stats}>
          {view.stats.map(cell => (
            <div key={cell.key} className={`lhc-stat is-${cell.group}${cell.derived ? " is-derived" : ""}${cell.key === "dmg" ? " is-dmg" : ""}${cell.value === UNKNOWN ? " is-unknown" : ""}`} title={statTitle(cell, text)}>
              <dt>{cell.label}</dt>
              <dd>{cell.value}{cell.gain !== null && <em>{cell.gain}</em>}</dd>
            </div>
          ))}
        </dl>
      </div>
    </>
  );
}

/** Own hero, full replay-style card. Keeps the last fresh loadout of the same context while the source is stale. */
export function OwnHeroCard({ locale, own, loadout, slot, stale, ageMs, hasLatestResult, headingId, headingLabel, gsiSetup, tile = false, listItem = false, fieldCard = false, wideStats = false, ownFieldMode, youLabel = "You" }: {
  locale: Locale; own: OwnHeroViewModel; loadout: LoadoutViewModel; slot: number | null; stale: boolean; ageMs: number | null;
  hasLatestResult: boolean; headingId: string; headingLabel: { title: string; latestResult: string };
  gsiSetup?: (replace: boolean) => Promise<string | null>;
  /** Tile layout is used only by the stacked second-monitor mode; the card keeps all replay sections and wraps them. */
  tile?: boolean;
  /** Wrap the card in a list item when it shares the second-monitor team list with the dense cards. */
  listItem?: boolean;
  /** Use the shared replay-card layout as one of the five team slots during a match. */
  fieldCard?: boolean;
  /** Use the wider two-row stat grid when the team column has room. */
  wideStats?: boolean;
  /** Select the own-card ability/item rows from the measured field card size. */
  ownFieldMode?: OwnFieldCardMode;
  youLabel?: string;
}) {
  const lang: Lang = locale === "en" ? "en" : "th";
  const text = heroCardCopy(lang);
  const [freshCache, setFreshCache] = useState<FreshLoadoutCache | null>(null);
  const presentationKey = JSON.stringify([loadout.source.generation, loadout.source.sessionEpoch, loadout.source.identityRevision,
    loadout.source.heroId, loadout.source.sequence, loadout.items.stale, loadout.items.slots, loadout.abilities.stale, loadout.abilities.tiles]);
  useEffect(() => { setFreshCache(previous => retainFreshLoadout(loadout, previous).cache); }, [presentationKey]);
  const presentation = retainFreshLoadout(loadout, freshCache);
  const view = ownFullCard({ own, items: presentation.items, itemsStale: loadout.items.stale, abilities: presentation.abilities,
    abilitiesStale: loadout.abilities.stale, slot, stale, icons: dotaIcon, statsData: LIVE_STATS_DATA, gameBuild: null });
  const style = (view.color ? { "--c": view.color } : {}) as CSSProperties;
  const ageText = `${stale ? text.oldData : text.age} · ${seconds(ageMs, locale)}`;
  const life = ownHeroLifeLabel(own, lang);
  if (fieldCard) {
    const field = (
      <li className={`lhc lhc-compact lhc-field-card lhc-field-own${wideStats ? " has-wide-stats" : ""}${ownFieldMode ? ` is-own-${ownFieldMode}` : ""}${stale ? " is-stale" : ""}${view.alive === false ? " is-dead" : ""}`}
        style={style} data-testid="live-hero-card-own" data-slot={slot ?? undefined} aria-label={`${youLabel}: ${view.heroName ?? UNKNOWN} · ${life}`}>
        <div className="lhc-pt">
          <CardPortrait heroId={view.heroId} src={view.portraitSrc} alt={view.heroName ?? text.unknown} />
          <span className="lhc-lv" title={`${text.level} ${view.levelText}`} aria-label={`${text.level} ${view.levelText}`}>{view.levelText}</span>
          {view.respawnText !== null && <span className="lhc-rsp" aria-hidden="true">{view.respawnText}</span>}
        </div>
        <div className="lhc-nm">
          <strong title={view.heroCaption ?? view.heroName ?? undefined}>{view.heroName ?? UNKNOWN}</strong>
          <span className="lhc-you-chip">{youLabel}</span>
          {!wideStats && <span className="lhc-life" title={life}>{life}</span>}
        </div>
        <CardSections view={view} text={text} locale={locale} abilitiesAge={loadout.abilities.ageMs} itemsAge={loadout.items.ageMs}
          basisInHeader={wideStats || ownFieldMode !== undefined} />
      </li>
    );
    return field;
  }
  const card = (
    <section className={`lhc lhc-full${tile ? " lhc-tile" : ""}${stale ? " is-stale" : ""}${view.alive === false ? " is-dead" : ""}`} style={style}
      aria-labelledby={headingId} data-testid="live-hero-card-own" data-slot={slot ?? undefined}>
      <header className="lhc-head">
        <h1 id={headingId}>{headingLabel.title}</h1>
        {hasLatestResult && <span className="lhc-result">{headingLabel.latestResult}</span>}
        <span className={`lhc-age${stale ? " is-stale" : ""}`} aria-label={ageText}>{ageText}</span>
      </header>
      <div className="lhc-pt">
        <CardPortrait heroId={view.heroId} src={view.portraitSrc} alt={view.heroName ?? text.unknown} />
        <span className="lhc-lv" title={`${text.level} ${view.levelText}`} aria-label={`${text.level} ${view.levelText}`}>{view.levelText}</span>
        {view.respawnText !== null && <span className="lhc-rsp" aria-hidden="true">{view.respawnText}</span>}
      </div>
      <div className="lhc-nm">
        <strong title={view.heroCaption ?? view.heroName ?? undefined}>{view.heroName ?? UNKNOWN}</strong>
        <span className="lhc-life" title={life}>{life}</span>
      </div>
      <CardSections view={view} text={text} locale={locale} abilitiesAge={loadout.abilities.ageMs} itemsAge={loadout.items.ageMs} />
      {loadout.setupHint && <div className="lhc-hint"><GsiSetupHint locale={lang} gsiSetup={gsiSetup} /></div>}
    </section>
  );

  return listItem ? <li className="lhc-list-item">{card}</li> : card;
}

/** Ally or enemy: the same sections in a denser variant; values without an admitted source are "—" or empty slots. */
function TeamCard({ view, locale, tile = false, fieldCard = false, wideStats = false, roomyField = false }: {
  view: FullHeroCardView; locale: Locale; tile?: boolean; fieldCard?: boolean; wideStats?: boolean; roomyField?: boolean;
}) {
  const lang: Lang = locale === "en" ? "en" : "th";
  const text = heroCardCopy(lang);
  const cards = cardCopy(lang);
  const style = (view.color ? { "--c": view.color } : {}) as CSSProperties;
  const who = view.side === "enemy" ? cards.enemyMark : cards.ourTeam;
  const note = view.heroName === null ? text.unnamedNote : view.side === "enemy" ? text.enemyNote : text.allyNote;
  return (
    <li className={`lhc lhc-compact${tile ? " lhc-tile" : ""}${fieldCard ? ` lhc-field-card${wideStats ? " has-wide-stats" : ""}${roomyField ? " is-roomy-field" : " is-compact-fit"}` : ""} is-${view.side}${view.heroName ? "" : " is-unknown"}`} style={style} title={note}
      data-testid={`live-hero-card-${view.key}`} data-slot={view.slot ?? undefined} aria-label={`${who}: ${view.heroName ?? UNKNOWN} · ${note}`}>
      <div className="lhc-pt">
        <CardPortrait heroId={view.heroId} src={view.portraitSrc} alt={view.heroName ?? UNKNOWN} />
        <span className="lhc-lv" aria-label={`${text.level} ${view.levelText}`}>{view.levelText}</span>
      </div>
      <div className="lhc-nm">
        <strong title={view.heroName ?? undefined}>{view.heroName ?? UNKNOWN}</strong>
        {view.ult && <span className={`lhc-ult is-${view.ult.state}`} title={`${cards.ult}: ${cards.state[view.ult.state]}`}
          aria-label={`${cards.ult}: ${cards.state[view.ult.state]}`} />}
      </div>
      <CardSections view={view} text={text} locale={locale} abilitiesAge={null} itemsAge={null} roomyField={roomyField} />
    </li>
  );
}

/** One dense replay-structure card for a mixed second-monitor team list. */
export function CompactHeroCardItem({ card, locale, tile = false, fieldCard = false, wideStats = false, roomyField = false }: {
  card: CardModel; locale: Locale; tile?: boolean; fieldCard?: boolean; wideStats?: boolean; roomyField?: boolean;
}) {
  return <TeamCard view={teamFullCard(card, dotaIcon)} locale={locale} tile={tile} fieldCard={fieldCard} wideStats={wideStats} roomyField={roomyField} />;
}

/** A team column of cards (four allies, or five enemies), each from the second-monitor card model. */
export function CompactHeroCards({ cards, locale, label, tile = false, fieldCard = false, fieldDirection = "column", wideStats = false, roomyField = false, renderOwn }: {
  cards: CardModel[]; locale: Locale; label: string; tile?: boolean; fieldCard?: boolean; wideStats?: boolean; roomyField?: boolean;
  fieldDirection?: "column" | "row";
  renderOwn?: (card: CardModel) => ReactNode;
}) {
  return (
    <ul className={`lhc-list${tile ? " lhc-list-tile" : ""}${fieldCard ? ` lhc-list-field lhc-list-field-${fieldDirection}` : ""}`} aria-label={label}>
      {cards.map(card => card.side === "own" && renderOwn
        ? <Fragment key={card.key}>{renderOwn(card)}</Fragment>
        : <CompactHeroCardItem key={card.key} card={card} locale={locale} tile={tile} fieldCard={fieldCard} wideStats={wideStats} roomyField={roomyField} />)}
    </ul>
  );
}
