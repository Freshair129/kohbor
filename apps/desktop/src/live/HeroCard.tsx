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
import { compactOwnFieldStats, ownFullCard, sharedHeroCardView, teamFullCard, UNKNOWN, type FullHeroCardView, type HeroCardLabelMode, type IconSlotView, type StatCellView } from "./heroCardModel";
import { statIconUrl } from "./statIcons/assets";
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

function Bar({ kind, percent, label, value, displayValue = value, low = false, showLabel = true, regenText = null, regenTitle = null }: {
  kind: "hp" | "mp"; percent: number | null; label: string; value: string; displayValue?: string; low?: boolean;
  showLabel?: boolean; regenText?: string | null; regenTitle?: string | null;
}) {
  const accessibleLabel = regenTitle ? `${label}: ${value}. ${regenTitle.split("\n")[0]}` : `${label}: ${value}`;
  return (
    <span className={`lhc-bar is-${kind}${low ? " is-low" : ""}${percent === null ? " is-unknown" : ""}`} role="img" aria-label={accessibleLabel} title={regenTitle ?? `${label}: ${value}`}>
      {percent !== null && <i style={{ width: `${percent}%` }} />}
      <span className={`lhc-bar-num${regenText !== null ? " has-regen" : ""}`}>{showLabel && <><small>{label}</small> </>}{displayValue}</span>
      {regenText !== null && <span className="lhc-bar-regen" aria-hidden="true">{regenText}</span>}
    </span>
  );
}

function regenTitle(kind: "hp" | "mp", textValue: string, sources: string[], basis: string | null, locale: Locale): string {
  const th = locale !== "en";
  const amount = textValue.startsWith("+") ? textValue.slice(1) : textValue;
  const description = kind === "hp"
    ? th ? `ฟื้นฟูพลังชีวิต ${amount} ต่อวินาที` : `HP regeneration ${amount} per second`
    : th ? `ฟื้นฟูมานา ${amount} ต่อวินาที` : `Mana regeneration ${amount} per second`;
  const estimate = th ? "ค่าคาดการณ์จากกฎคำนวณ DERIVED" : "Derived estimate from the stat rules";
  const caveat = "ไม่รวมผลชั่วคราว / without temporary effects";
  return `${description}\n${estimate}\nSources: ${sources.length > 0 ? sources.join("; ") : "unknown"}\n${caveat}${basis ? `\n${basis}` : ""}`;
}

function statTitle(cell: StatCellView, text: Text): string {
  if (cell.derived) {
    const sources = cell.sources.length > 0 ? cell.sources.join("; ") : "unknown";
    return `${cell.accessibleLabel}: ${cell.value}\nSources: ${sources}\nไม่รวมผลชั่วคราว / without temporary effects.${cell.basis ? `\n${cell.basis}` : ""}`;
  }
  return cell.value === UNKNOWN && cell.group !== "rate"
    ? `${cell.accessibleLabel}: ${UNKNOWN} (${text.statNoSource})`
    : `${cell.accessibleLabel}: ${cell.value}`;
}

function StatGlyphs({ cell, labelMode }: { cell: StatCellView; labelMode: HeroCardLabelMode }) {
  return (
    <dt data-stat-label={labelMode} role="img" aria-label={cell.accessibleLabel} title={cell.accessibleLabel}>
      {labelMode === "text" ? <span>{cell.label}</span> : cell.iconIds.map(iconId => {
        const src = statIconUrl(iconId);
        return src
          ? <img key={iconId} src={src} alt="" aria-hidden="true" draggable={false} />
          : <span key={iconId} className="lhc-stat-icon-fallback" aria-hidden="true">{cell.label}</span>;
      })}
    </dt>
  );
}

/** The sections every card shares, in the replay card's order. */
function CardSections({ view, text, locale, abilitiesAge, itemsAge, statLabelMode = "icon", basisInHeader = false, roomyField = false, ownField = false, compactOwnField = false }: {
  view: FullHeroCardView; text: Text; locale: Locale; abilitiesAge: number | null; itemsAge: number | null; basisInHeader?: boolean; roomyField?: boolean;
  ownField?: boolean; compactOwnField?: boolean; statLabelMode?: HeroCardLabelMode;
}) {
  const { abilities, items } = view;
  const displayedStats = compactOwnField ? compactOwnFieldStats(view.stats) : view.stats;
  const sharedRows = sharedHeroCardView(view).rows;
  const attributeStats = displayedStats.filter(cell => cell.group === "attribute");
  const derivedStats = displayedStats.filter(cell => cell.group === "derived");
  return (
    <>
      <div className="lhc-top" data-card-row={sharedRows[0]!.id}>
        <span className="lhc-kd" title={`${text.kda}: ${view.kdaText}`}>{view.kdaText}</span>
        {!roomyField && <span title={`${text.lhdn}: ${view.lhdnText}`}>{view.lhdnText}</span>}
        {!roomyField && <span className="lhc-gold" title={`${text.gold}: ${view.goldText}`}>{view.goldText}</span>}
        {view.side === "own" && <span className="lhc-rates" title={text.rates}>{text.rates} {view.ratesText}</span>}
      </div>
      {roomyField && <div className="lhc-econ">
        <span title={`${text.lhdn}: ${view.lhdnText}`}>{view.lhdnText}</span>
        <span className="lhc-gold" title={`${text.gold}: ${view.goldText}`}>{view.goldText}</span>
      </div>}
      <div className="lhc-bars" data-card-row={sharedRows[1]!.id}>
        <Bar kind="hp" percent={view.hpPercent} label={text.hp} value={view.hpText}
          displayValue={ownField ? view.hpText.replace(/\s*\/\s*/g, "/") : view.hpText} showLabel={!ownField} low={view.hpLow}
          regenText={view.hpRegenText} regenTitle={view.hpRegenText === null ? null : regenTitle("hp", view.hpRegenText, view.hpRegenSources, view.derivedBasis?.title ?? null, locale)} />
        <Bar kind="mp" percent={view.manaPercent} label={text.mana} value={view.manaText} displayValue={view.manaText} showLabel={!ownField}
          regenText={view.manaRegenText} regenTitle={view.manaRegenText === null ? null : regenTitle("mp", view.manaRegenText, view.manaRegenSources, view.derivedBasis?.title ?? null, locale)} />
      </div>
      <div className={`lhc-row lhc-ab${abilities.stale ? " is-stale" : ""}`} data-card-row={sharedRows[2]!.id} aria-label={text.abilities}
        title={abilities.stale ? `${text.oldData} · ${seconds(abilitiesAge, locale)}` : abilities.known ? undefined : `${text.abilities}: ${text.unknown}`}>
        {abilities.hero.map((slot, index) => <Icon key={slot.key} slot={slot} shape="ab" place={`${text.abilities} ${index + 1}`} text={text} stale={abilities.stale} />)}
        {abilities.extras.length > 0 && <span className="lhc-extras" aria-label={text.extras}>
          {abilities.extras.map(slot => <Icon key={slot.key} slot={slot} shape="ab" place={text.extras} text={text} stale={abilities.stale} />)}
        </span>}
        <span className="lhc-mods" aria-label={text.buffs} title={view.buffs.length === 0 ? text.buffsNone : undefined}>
          {view.buffs.map(buff => <span key={buff} className="lhc-mod">{buff}</span>)}
        </span>
      </div>
      <div className={`lhc-row lhc-it${items.stale ? " is-stale" : ""}`} data-card-row={sharedRows[3]!.id} aria-label={text.items}
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
        {attributeStats.length > 0 && <dl className="lhc-stats is-attributes" data-card-row={sharedRows[4]!.id} aria-label={text.stats}>
          {attributeStats.map(cell => (
            <div key={cell.key} className={`lhc-stat is-${cell.group} is-${cell.key}${cell.derived ? " is-derived" : ""}${cell.key === "dmg" ? " is-dmg" : ""}${cell.value === UNKNOWN ? " is-unknown" : ""}`} title={statTitle(cell, text)}>
              <StatGlyphs cell={cell} labelMode={statLabelMode} />
              <dd>{cell.value}{cell.gain !== null && <em>{cell.gain}</em>}</dd>
            </div>
          ))}
        </dl>}
        {derivedStats.length > 0 && <dl className={`lhc-stats is-derived-row${compactOwnField ? " is-own-compact" : ""}`} data-card-row={sharedRows[5]!.id} aria-label={text.stats}>
          {derivedStats.map(cell => (
            <div key={cell.key} className={`lhc-stat is-${cell.group} is-${cell.key}${cell.derived ? " is-derived" : ""}${cell.value === UNKNOWN ? " is-unknown" : ""}`} title={statTitle(cell, text)}>
              <StatGlyphs cell={cell} labelMode={statLabelMode} />
              <dd>{cell.value}{cell.gain !== null && <em>{cell.gain}</em>}</dd>
            </div>
          ))}
        </dl>}
      </div>
    </>
  );
}

/** Live renderer for the canonical six-row card; Replay and Analytics use the same row IDs and shared CSS. */
function SharedLiveHeroCard({ view, locale, statLabelMode = "icon", className = "", title }: {
  view: FullHeroCardView; locale: Locale; statLabelMode?: HeroCardLabelMode; className?: string; title?: string;
}) {
  const text = heroCardCopy(locale === "en" ? "en" : "th");
  const rows = sharedHeroCardView(view).rows;
  const style = { "--c": view.color ?? (view.side === "enemy" ? "var(--enemy)" : "var(--ally)") } as CSSProperties;
  const attrs = (index: number) => ({ "data-card-row": rows[index]!.id });
  const byKey = new Map(view.stats.map(cell => [cell.key, cell]));
  const kdText = view.kdaText === UNKNOWN ? UNKNOWN : view.kdaText.split("/").slice(0, 2).join("/");
  const StatLabel = ({ cell }: { cell: StatCellView }) => <span className="stat-glyph" data-stat-label={statLabelMode} role="img" aria-label={cell.accessibleLabel}>
    {statLabelMode === "text" ? cell.label : cell.iconIds.map(iconId => {
      const src = statIconUrl(iconId);
      return src ? <img key={iconId} src={src} alt="" aria-hidden="true" draggable={false} /> : <span key={iconId}>{cell.label}</span>;
    })}
  </span>;
  const topStat = (key: StatCellView["key"], classes = "") => {
    const cell = byKey.get(key);
    if (!cell) return <span key={key} className={`at ${classes}`}><b>—</b></span>;
    const value = key === "reg" && view.side === "own" ? `${cell.value}/${view.manaRegenText ?? UNKNOWN}` : cell.value;
    return <span key={key} className={`at ${classes}${cell.key === "dmg" ? " is-dmg" : ""}`} title={statTitle(cell, text)}>
      <StatLabel cell={cell} /> <b>{value}</b>{cell.gain !== null && <em>{cell.gain}</em>}
    </span>;
  };
  const derivedStat = (key: StatCellView["key"]) => {
    const cell = byKey.get(key);
    if (!cell) return <span key={key} className="dv"><b>—</b></span>;
    return <span key={key} className={`dv${cell.value === "0" || cell.value === "0%" ? " z" : ""}`} title={statTitle(cell, text)}>
      <StatLabel cell={cell} /> <b>{cell.value}</b>
    </span>;
  };
  const bar = (kind: "hp" | "mp", value: string, percent: number | null, low = false) => <div className={`bar ${kind}${low ? " low" : ""}`} title={`${kind === "hp" ? text.hp : text.mana}: ${value}`}>
    {percent !== null && <i style={{ width: `${percent}%` }} />}
    <span className="num">{value.replace(/\s*\/\s*/g, " / ")}</span>
  </div>;
  const slotIcon = (slot: IconSlotView, shape: "ab" | "it" | "bk" | "tp" | "nt", place: string, stale: boolean) =>
    <Icon key={slot.key} slot={slot} shape={shape} place={place} text={text} stale={stale} />;
  return <li className={`rc lhc-live${view.side === "enemy" ? " is-enemy" : view.side === "own" ? " is-own" : " is-ally"}${view.alive === false ? " dead" : ""}${view.stale ? " is-stale" : ""} ${className}`.trim()}
    style={style} title={title} data-testid={`live-hero-card-${view.key}`} data-slot={view.slot ?? undefined}
    aria-label={`${view.heroName ?? UNKNOWN} · ${view.kdaText} · ${view.lhdnText}`}>
    <div className="pt"><CardPortrait heroId={view.heroId} src={view.portraitSrc} alt={view.heroName ?? text.unknown} />
      <span className="lvb">{view.levelText}</span>{view.respawnText !== null && <span className="rsp">{view.respawnText}</span>}</div>
    <div className="top" {...attrs(0)}>
      <span className="nm" title={view.heroName ?? undefined}>{view.heroName ?? UNKNOWN}</span>
      <span className="kd num" title={text.kda}>{kdText}</span>
      <span className="num" title={text.lhdn}>{view.lhdnText}</span>
      <span className="num nw" title="Net worth: unknown">{UNKNOWN}</span>
    </div>
    <div className="bars" {...attrs(1)}>{bar("hp", view.hpText, view.hpPercent, view.hpLow)}{bar("mp", view.manaText, view.manaPercent)}</div>
    <div className="row ab" {...attrs(2)}>
      {view.abilities.hero.map((slot, index) => slotIcon(slot, "ab", `${text.abilities} ${index + 1}`, view.abilities.stale))}
      {view.abilities.extras.map((slot, index) => slotIcon(slot, "ab", `${text.extras} ${index + 1}`, view.abilities.stale))}
      <span className="lhc-mods">{view.buffs.slice(0, 3).map((buff, index) => <span className="lhc-mod" key={`${index}-${buff}`}>{buff}</span>)}</span>
    </div>
    <div className="row it" {...attrs(3)}>
      {view.items.main.map((slot, index) => slotIcon(slot, "it", text.slot(index + 1), view.items.stale))}
      <span className="bp">{view.items.backpack.map((slot, index) => slotIcon(slot, "bk", `${text.backpack} ${index + 1}`, view.items.stale))}</span>
      {slotIcon(view.items.teleport, "tp", text.teleport, view.items.stale)}
      {slotIcon(view.items.neutral, "nt", text.neutral, view.items.stale)}
      <span className="gold num" title={text.gold}>{view.side === "own" ? view.goldText : UNKNOWN}</span>
    </div>
    <div className="row st" {...attrs(4)}>{(["str", "agi", "int", "dmg", "reg"] as const).map(key => topStat(key))}</div>
    <div className="row dvs" {...attrs(5)}>{(["arm", "mr", "amp", "as", "ms", "ev", "sr", "ls"] as const).map(derivedStat)}</div>
  </li>;
}

/** Own hero, full replay-style card. Keeps the last fresh loadout of the same context while the source is stale. */
export function OwnHeroCard({ locale, own, loadout, slot, stale, ageMs, hasLatestResult, headingId, headingLabel, gsiSetup, tile = false, listItem = false, fieldCard = false, youLabel = "You", statLabelMode = "icon" }: {
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
  youLabel?: string; statLabelMode?: HeroCardLabelMode;
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
    return <SharedLiveHeroCard view={view} locale={locale} statLabelMode={statLabelMode} className="live-own-card" title={`${youLabel}: ${view.heroName ?? UNKNOWN} · ${life}`} />;
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
      <CardSections view={view} text={text} locale={locale} abilitiesAge={loadout.abilities.ageMs} itemsAge={loadout.items.ageMs} statLabelMode={statLabelMode} />
      {loadout.setupHint && <div className="lhc-hint"><GsiSetupHint locale={lang} gsiSetup={gsiSetup} /></div>}
    </section>
  );

  return listItem ? <li className="lhc-list-item">{card}</li> : card;
}

/** Ally or enemy: the same sections in a denser variant; values without an admitted source are "—" or empty slots. */
function TeamCard({ view, locale, tile = false, fieldCard = false, wideStats = false, roomyField = false, statLabelMode = "icon" }: {
  view: FullHeroCardView; locale: Locale; tile?: boolean; fieldCard?: boolean; wideStats?: boolean; roomyField?: boolean; statLabelMode?: HeroCardLabelMode;
}) {
  const lang: Lang = locale === "en" ? "en" : "th";
  const text = heroCardCopy(lang);
  const cards = cardCopy(lang);
  const style = (view.color ? { "--c": view.color } : {}) as CSSProperties;
  const who = view.side === "enemy" ? cards.enemyMark : cards.ourTeam;
  const note = view.heroName === null ? text.unnamedNote : view.side === "enemy" ? text.enemyNote : text.allyNote;
  if (fieldCard) return <SharedLiveHeroCard view={view} locale={locale} statLabelMode={statLabelMode} className="live-team-card" title={note} />;
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
      <CardSections view={view} text={text} locale={locale} abilitiesAge={null} itemsAge={null} roomyField={roomyField} statLabelMode={statLabelMode} />
    </li>
  );
}

/** One dense replay-structure card for a mixed second-monitor team list. */
export function CompactHeroCardItem({ card, locale, tile = false, fieldCard = false, wideStats = false, roomyField = false, statLabelMode = "icon" }: {
  card: CardModel; locale: Locale; tile?: boolean; fieldCard?: boolean; wideStats?: boolean; roomyField?: boolean; statLabelMode?: HeroCardLabelMode;
}) {
  return <TeamCard view={teamFullCard(card, dotaIcon)} locale={locale} tile={tile} fieldCard={fieldCard} wideStats={wideStats} roomyField={roomyField} statLabelMode={statLabelMode} />;
}

/** A team column of cards (four allies, or five enemies), each from the second-monitor card model. */
export function CompactHeroCards({ cards, locale, label, tile = false, fieldCard = false, fieldDirection = "column", wideStats = false, roomyField = false, renderOwn, statLabelMode = "icon" }: {
  cards: CardModel[]; locale: Locale; label: string; tile?: boolean; fieldCard?: boolean; wideStats?: boolean; roomyField?: boolean;
  fieldDirection?: "column" | "row";
  renderOwn?: (card: CardModel) => ReactNode; statLabelMode?: HeroCardLabelMode;
}) {
  return (
    <ul className={`lhc-list${tile ? " lhc-list-tile" : ""}${fieldCard ? ` lhc-list-field lhc-list-field-${fieldDirection}` : ""}`} aria-label={label}>
      {cards.map(card => card.side === "own" && renderOwn
        ? <Fragment key={card.key}>{renderOwn(card)}</Fragment>
        : <CompactHeroCardItem key={card.key} card={card} locale={locale} tile={tile} fieldCard={fieldCard} wideStats={wideStats} roomyField={roomyField} statLabelMode={statLabelMode} />)}
    </ul>
  );
}
