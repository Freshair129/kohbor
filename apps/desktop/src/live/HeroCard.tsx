// Replay-style hero cards for the centred Live deck (DEC-REPLAY-INTEGRATION item 10, slice R5).
// Full variant: the own hero from GSI. Compact variant: allies and enemies from the second-monitor card model.
// Unknown renders "—"; no attribute/derived stat rows, no path traces, no enemy value without an admitted source.
import { useEffect, useState, type CSSProperties } from "react";
import { seconds, type Locale } from "../copy";
import { HeroPortrait } from "../deck/HeroPortrait";
import { GsiSetupHint } from "../deck/OwnLoadout";
import { retainFreshLoadout, type FreshLoadoutCache, type LoadoutViewModel } from "../deck/loadoutModel";
import { ownHeroLifeLabel, type OwnHeroViewModel } from "../deck/ownHeroModel";
import { cardCopy, type HeroCard as CardModel } from "./secondMonitorCards";
import { dotaIcon } from "./dotaIconSet";
import { compactCard, ownFullCard, UNKNOWN, type CompactHeroCardView, type IconSlotView } from "./heroCardModel";
import "./heroCard.css";

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
    items: th ? "ไอเท็ม" : "Items",
    backpack: th ? "ช่องเป้" : "Backpack",
    teleport: th ? "ช่องวาร์ป" : "Teleport slot",
    neutral: th ? "ช่องไอเท็มกลาง" : "Neutral item slot",
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

/** Bundled icon when one exists, else the runtime portrait from the installed game, else the neutral initials tile. */
function CardPortrait({ heroId, src, alt }: { heroId: string | null; src: string | null; alt: string }) {
  const [failed, setFailed] = useState<string | null>(null);
  if (src && failed !== src) {
    return <span className="lhc-portrait"><img src={src} alt={alt} decoding="async" draggable={false} onError={() => setFailed(src)} /></span>;
  }
  return <span className="lhc-portrait"><HeroPortrait heroId={heroId} size="md" alt={alt} /></span>;
}

function iconTitle(slot: IconSlotView, place: string, text: ReturnType<typeof heroCardCopy>, stale: boolean): string {
  const parts = [place];
  if (slot.state === "unknown") parts.push(text.unknown);
  else if (slot.state === "empty") parts.push(text.empty);
  else {
    parts.push(`${slot.name ?? UNKNOWN}${slot.rawId ? ` (${slot.rawId})` : ""}`);
    if (slot.level !== null) parts.push(`${text.level} ${slot.level}`);
    if (slot.cast === "not_learned") parts.push(text.notLearned);
    else if (slot.cast === "cooldown" && slot.cooldownText) parts.push(`${text.cooldown} ${slot.cooldownText}s`);
    else if (slot.cast === "ready") parts.push(text.ready);
    if (slot.charges !== null) parts.push(`${text.charges} ${slot.charges}`);
  }
  if (stale) parts.push(text.oldData);
  return parts.join(" · ");
}

function Icon({ slot, shape, place, text, stale }: {
  slot: IconSlotView; shape: "ab" | "it" | "bk" | "tp" | "nt"; place: string; text: ReturnType<typeof heroCardCopy>; stale: boolean;
}) {
  const [failed, setFailed] = useState<string | null>(null);
  const image = slot.src && failed !== slot.src ? slot.src : null;
  const title = iconTitle(slot, place, text, stale);
  const classes = ["lhc-ic", `is-${shape}`, `is-${slot.state}`, `cast-${slot.cast}`, slot.ultimate ? "is-ult" : "", image ? "has-img" : "", stale ? "is-stale" : ""]
    .filter(Boolean).join(" ");
  return (
    <span className={classes} title={title} aria-label={title} role="img">
      {image && <img src={image} alt="" draggable={false} onError={() => setFailed(image)} />}
      {!image && slot.state !== "empty" && <b aria-hidden="true">{slot.state === "unknown" ? "?" : slot.text}</b>}
      {slot.cooldownText && <s aria-hidden="true">{slot.cooldownText}</s>}
      {slot.charges !== null && <em aria-hidden="true">{slot.charges}</em>}
      {slot.maxLevel !== null && slot.level !== null && slot.level > 0 && (
        <i className="lhc-pips" aria-hidden="true">{Array.from({ length: Math.min(slot.level, slot.maxLevel) }, (_, i) => <span key={i} />)}</i>
      )}
    </span>
  );
}

function Placeholder({ count, shape, label }: { count: number; shape: "ab" | "it"; label: string }) {
  return (
    <span className="lhc-placeholder" role="img" aria-label={label} title={label}>
      {Array.from({ length: count }, (_, i) => <span key={i} className={`lhc-ic is-${shape} is-placeholder`} aria-hidden="true" />)}
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

/** Own hero, full replay-style card. Keeps the last fresh loadout of the same context while the source is stale. */
export function OwnHeroCard({ locale, own, loadout, slot, stale, ageMs, hasLatestResult, headingId, headingLabel, gsiSetup }: {
  locale: Locale; own: OwnHeroViewModel; loadout: LoadoutViewModel; slot: number | null; stale: boolean; ageMs: number | null;
  hasLatestResult: boolean; headingId: string; headingLabel: { title: string; latestResult: string };
  gsiSetup?: (replace: boolean) => Promise<string | null>;
}) {
  const lang: Lang = locale === "en" ? "en" : "th";
  const text = heroCardCopy(lang);
  const [freshCache, setFreshCache] = useState<FreshLoadoutCache | null>(null);
  const presentationKey = JSON.stringify([loadout.source.generation, loadout.source.sessionEpoch, loadout.source.identityRevision,
    loadout.source.heroId, loadout.source.sequence, loadout.items.stale, loadout.items.slots, loadout.abilities.stale, loadout.abilities.tiles]);
  useEffect(() => { setFreshCache(previous => retainFreshLoadout(loadout, previous).cache); }, [presentationKey]);
  const presentation = retainFreshLoadout(loadout, freshCache);
  const view = ownFullCard({ own, items: presentation.items, itemsStale: loadout.items.stale, abilities: presentation.abilities,
    abilitiesStale: loadout.abilities.stale, slot, stale, icons: dotaIcon });
  const style = (view.color ? { "--c": view.color } : {}) as CSSProperties;
  const ageText = `${stale ? text.oldData : text.age} · ${seconds(ageMs, locale)}`;
  const life = ownHeroLifeLabel(own, lang);

  return (
    <section className={`lhc lhc-full${stale ? " is-stale" : ""}${view.alive === false ? " is-dead" : ""}`} style={style}
      aria-labelledby={headingId} data-testid="live-hero-card-own">
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
      <div className="lhc-top">
        <span className="lhc-kd" title={`${text.kda}: ${view.kdaText}`}>{view.kdaText}</span>
        <span title={`${text.lhdn}: ${view.lhdnText}`}>{view.lhdnText}</span>
        <span className="lhc-gold" title={`${text.gold}: ${view.goldText}`}>{view.goldText}</span>
      </div>
      <div className="lhc-bars">
        <Bar kind="hp" percent={view.hpPercent} label={text.hp} value={view.hpText} low={view.hpLow} />
        <Bar kind="mp" percent={view.manaPercent} label={text.mana} value={view.manaText} />
      </div>
      <div className={`lhc-row lhc-ab${view.abilities?.stale ? " is-stale" : ""}`} aria-label={text.abilities}
        title={view.abilities?.stale ? `${text.oldData} · ${seconds(loadout.abilities.ageMs, locale)}` : undefined}>
        {view.abilities ? <>
          {view.abilities.hero.map(slotView => <Icon key={slotView.key} slot={slotView} shape="ab" place={text.abilities} text={text} stale={view.abilities!.stale} />)}
          {view.abilities.extras.length > 0 && <span className="lhc-extras" aria-label={text.extras}>
            {view.abilities.extras.map(slotView => <Icon key={slotView.key} slot={slotView} shape="ab" place={text.extras} text={text} stale={view.abilities!.stale} />)}
          </span>}
        </> : <Placeholder count={4} shape="ab" label={text.noData} />}
      </div>
      <div className={`lhc-row lhc-it${view.items?.stale ? " is-stale" : ""}`} aria-label={text.items}
        title={view.items?.stale ? `${text.oldData} · ${seconds(loadout.items.ageMs, locale)}` : undefined}>
        {view.items ? <>
          <span className="lhc-main">
            {view.items.main.map((slotView, index) => <Icon key={slotView.key} slot={slotView} shape="it" place={text.slot(index + 1)} text={text} stale={view.items!.stale} />)}
          </span>
          <Icon slot={view.items.neutral} shape="nt" place={text.neutral} text={text} stale={view.items.stale} />
          <Icon slot={view.items.teleport} shape="tp" place={text.teleport} text={text} stale={view.items.stale} />
        </> : <Placeholder count={6} shape="it" label={text.noData} />}
      </div>
      <div className="lhc-row lhc-st">
        {view.items && <span className="lhc-bp" aria-label={text.backpack}>
          {view.items.backpack.map((slotView, index) => <Icon key={slotView.key} slot={slotView} shape="bk" place={`${text.backpack} B${index + 1}`} text={text} stale={view.items!.stale} />)}
        </span>}
        <span className="lhc-rates" title={`${text.rates}: ${view.ratesText}`}><small>{text.rates}</small> {view.ratesText}</span>
      </div>
      {loadout.setupHint && <div className="lhc-hint"><GsiSetupHint locale={lang} gsiSetup={gsiSetup} /></div>}
    </section>
  );
}

function CompactCard({ view, locale }: { view: CompactHeroCardView; locale: Lang }) {
  const text = heroCardCopy(locale);
  const cards = cardCopy(locale);
  const style = (view.color ? { "--c": view.color } : {}) as CSSProperties;
  const who = view.side === "enemy" ? cards.enemyMark : cards.ourTeam;
  const note = view.heroName === null ? text.unnamedNote : view.side === "enemy" ? text.enemyNote : text.allyNote;
  const vitals = view.vitalsSource !== null;
  return (
    <li className={`lhc lhc-compact is-${view.side}${view.heroName ? "" : " is-unknown"}`} style={style}
      data-testid={`live-hero-card-${view.key}`} data-slot={view.slot ?? undefined} aria-label={`${who}: ${view.heroName ?? UNKNOWN}`}>
      <div className="lhc-pt">
        <CardPortrait heroId={view.heroId} src={view.portraitSrc} alt={view.heroName ?? UNKNOWN} />
        <span className="lhc-lv" aria-label={`${text.level} ${UNKNOWN}`}>{UNKNOWN}</span>
      </div>
      <div className="lhc-cbody">
        <div className="lhc-ctop">
          <strong title={view.heroName ?? undefined}>{view.heroName ?? UNKNOWN}</strong>
          {view.ult && <span className={`lhc-ult is-${view.ult.state}`} title={`${cards.ult}: ${cards.state[view.ult.state]}`}
            aria-label={`${cards.ult}: ${cards.state[view.ult.state]}`} />}
        </div>
        {vitals ? <div className="lhc-cbars">
          <Bar kind="hp" percent={view.hpPercent} label={text.hp} value={view.hpPercent === null ? UNKNOWN : `${Math.round(view.hpPercent)}%`} />
          <Bar kind="mp" percent={view.manaPercent} label={text.mana} value={view.manaPercent === null ? UNKNOWN : `${Math.round(view.manaPercent)}%`} />
        </div> : <small className="lhc-cnote" title={note}>{note}</small>}
      </div>
    </li>
  );
}

/** A team column of compact cards (four allies, or five enemies), each from the second-monitor card model. */
export function CompactHeroCards({ cards, locale, label }: { cards: CardModel[]; locale: Locale; label: string }) {
  const lang: Lang = locale === "en" ? "en" : "th";
  return (
    <ul className="lhc-list" aria-label={label}>
      {cards.map(card => <CompactCard key={card.key} view={compactCard(card, dotaIcon)} locale={lang} />)}
    </ul>
  );
}
