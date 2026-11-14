import { seconds } from "../copy";
import { abilityStatusText, type ItemTile, type LoadoutViewModel } from "./loadoutModel";
import "./loadout.css";

function ItemSlotTile({ tile, small, locale }: { tile: ItemTile; small?: boolean; locale: "th" | "en" }) {
  const empty = locale === "th" ? "ว่าง" : "Empty";
  const name = tile.kind === "empty" ? empty : tile.name ?? "—";
  const title = `${tile.slot}: ${name}${tile.rawId ? ` (${tile.rawId})` : ""}`;
  const slot = tile.slot.startsWith("backpack") ? `B${Number(tile.slot.at(-1)) + 1}` : tile.slot === "teleport" ? "TP" : tile.slot === "neutral" ? "N" : null;
  return <div className={`deck-item-tile is-${tile.kind}${small ? " is-small" : ""}`} title={title} aria-label={title}>
    {small && <small className="deck-item-slot">{slot}</small>}<span className="deck-item-name">{name}</span>
    <span className="deck-item-badges">
      {tile.cooldown !== null && <b title={`${locale === "th" ? "คูลดาวน์" : "Cooldown"}: ${tile.cooldown}s`}>{tile.cooldown}s</b>}
      {tile.charges !== null && <b title={`${locale === "th" ? "จำนวนชาร์จ" : "Charges"}: ${tile.charges}`}>×{tile.charges}</b>}
    </span>
  </div>;
}

export function OwnLoadout({ model, locale }: { model: LoadoutViewModel; locale: "th" | "en" }) {
  const th = locale === "th";
  const old = (age: number | null) => `${th ? "ข้อมูลเก่า" : "Old data"} · ${seconds(age, locale)}`;
  return <section className="deck-loadout" aria-label={th ? "ไอเท็มและสกิล" : "Items and abilities"}>
    <section className={`deck-loadout-section${model.items.stale ? " is-stale" : ""}`} aria-label={th ? "ไอเท็ม" : "Items"}>
      <header><h2>{th ? "ไอเท็ม" : "Items"}</h2>{model.items.stale && <small>{old(model.items.ageMs)}</small>}</header>
      {model.items.slots ? <>
        <div className="deck-inventory">{model.items.slots.slice(0, 6).map(tile => <ItemSlotTile key={tile.slot} tile={tile} locale={locale} />)}</div>
        <div className="deck-extra-items">{model.items.slots.slice(6).map(tile => <ItemSlotTile key={tile.slot} tile={tile} small locale={locale} />)}</div>
      </> : <span className="deck-loadout-dash">—</span>}
    </section>
    <section className={`deck-loadout-section${model.abilities.stale ? " is-stale" : ""}`} aria-label={th ? "สกิลและคูลดาวน์" : "Abilities and cooldowns"}>
      <header><h2>{th ? "สกิล · คูลดาวน์" : "Abilities · cooldowns"}</h2>{model.abilities.stale && <small>{old(model.abilities.ageMs)}</small>}</header>
      {model.abilities.tiles?.length ? <div className="deck-ability-grid">{model.abilities.tiles.map(tile => (
        <div key={tile.slot} className={`deck-ability-tile${tile.status === "ready" ? " is-ready" : ""}${tile.ultimate ? " is-ultimate" : ""}`}
          title={`${tile.name} (${tile.rawId}) · ${th ? "เลเวล" : "Level"} ${tile.level ?? "—"}`}>
          <strong>{tile.name}</strong>{tile.ultimate && <b className="deck-ultimate-tag">ULT</b>}
          <div className="deck-ability-detail"><span className="deck-level-pips" aria-label={`${th ? "เลเวล" : "Level"}: ${tile.level ?? "—"}`}>
            {tile.pips ? tile.pips.map((learned, index) => <i key={index} className={learned ? "is-learned" : undefined} aria-hidden="true">{learned ? "●" : "○"}</i>) : "—"}
          </span><span className="deck-ability-status" title={abilityStatusText(tile, locale)}>{abilityStatusText(tile, locale)}</span></div>
        </div>
      ))}</div> : <span className="deck-loadout-dash">—</span>}
    </section>
    {model.setupHint && <small className="deck-stats-caption deck-setup-hint">
      <span>{th ? "ต้องตั้งค่าการเชื่อมต่อ Dota 2 ใหม่หนึ่งครั้ง" : "Run Dota 2 connection setup once again"}</span>
      <span>{th ? "เกมที่เปิดก่อนเพิ่มบล็อก items/abilities ใน cfg จะยังไม่ส่งข้อมูลส่วนนี้" : "A game started before the cfg gained items/abilities blocks will not send them yet."}</span>
    </small>}
  </section>;
}
