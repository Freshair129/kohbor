import { useEffect, useState } from "react";
import type { CSSProperties } from "react";
import { reason, seconds } from "../copy";
import { abilityStatusText, groupItemSlots, retainFreshLoadout, splitAbilityRows, type AbilityTile, type FreshLoadoutCache, type ItemTile, type LoadoutViewModel } from "./loadoutModel";
import { cooldownTickMode, cooldownTrackKey, estimateCooldown, formatCooldownValue, trackCooldown, type CooldownState } from "./cooldownModel";
import "./loadout.css";

type CooldownTracks = Record<string, CooldownState>;
type CooldownInput = { key: string; remaining: number | null; ready?: boolean };

function updateCooldownSection(next: CooldownTracks, previous: CooldownTracks, prefix: "item" | "ability",
  stale: boolean, inputs: CooldownInput[] | null, nowMs: number, reportedAtMs: number) {
  const keyPrefix = `${prefix}:`;
  if (stale) {
    for (const key of Object.keys(next)) {
      if (key.startsWith(keyPrefix)) next[key] = trackCooldown(previous[key] ?? null, { remaining: null, stale: true }, nowMs);
    }
    return;
  }
  if (inputs === null) {
    for (const key of Object.keys(next)) if (key.startsWith(keyPrefix)) delete next[key];
    return;
  }
  const retained = new Set<string>();
  for (const input of inputs) {
    retained.add(input.key);
    next[input.key] = trackCooldown(previous[input.key] ?? null,
      { remaining: input.remaining, ready: input.ready }, reportedAtMs);
  }
  for (const key of Object.keys(next)) {
    if (key.startsWith(keyPrefix) && !retained.has(key)) delete next[key];
  }
}

function useCooldownClock(model: LoadoutViewModel) {
  const [tracks, setTracks] = useState<CooldownTracks>({});
  const [nowMs, setNowMs] = useState(() => performance.now());
  const [visible, setVisible] = useState(() => typeof document === "undefined" || document.visibilityState === "visible");
  const [reducedMotion, setReducedMotion] = useState(() => typeof window !== "undefined"
    && window.matchMedia?.("(prefers-reduced-motion: reduce)").matches === true);

  const contextKey = JSON.stringify([model.source.generation, model.source.sessionEpoch,
    model.source.identityRevision, model.source.heroId]);
  const reportKey = JSON.stringify({
    source: [contextKey, model.source.sequence, model.source.ageMs],
    items: [model.items.stale, model.items.slots?.map(tile => [tile.slot, tile.rawId, tile.cooldown]) ?? null],
    abilities: [model.abilities.stale, model.abilities.tiles?.map(tile =>
      [tile.slot, tile.rawId, tile.cooldown, tile.status, tile.level, tile.canCast]) ?? null],
  });

  useEffect(() => {
    const now = performance.now();
    const reportedAt = now - Math.max(0, model.source.ageMs ?? 0);
    setTracks(previous => {
      const next = { ...previous };
      updateCooldownSection(next, previous, "item", model.items.stale,
        model.items.slots?.map(tile => ({ key: cooldownTrackKey("item", contextKey, tile.slot, tile.rawId), remaining: tile.cooldown })) ?? null,
        now, reportedAt);
      updateCooldownSection(next, previous, "ability", model.abilities.stale,
        model.abilities.tiles?.map(tile => ({ key: cooldownTrackKey("ability", contextKey, tile.slot, tile.rawId), remaining: tile.cooldown,
          ready: tile.status === "ready" })) ?? null, now, reportedAt);
      return next;
    });
  }, [reportKey]);

  useEffect(() => {
    if (typeof document === "undefined") return;
    const syncVisibility = () => setVisible(document.visibilityState === "visible");
    document.addEventListener("visibilitychange", syncVisibility);
    syncVisibility();
    return () => document.removeEventListener("visibilitychange", syncVisibility);
  }, []);

  useEffect(() => {
    if (typeof window === "undefined" || !window.matchMedia) return;
    const media = window.matchMedia("(prefers-reduced-motion: reduce)");
    const syncMotion = () => setReducedMotion(media.matches);
    media.addEventListener?.("change", syncMotion);
    syncMotion();
    return () => media.removeEventListener?.("change", syncMotion);
  }, []);

  const mode = cooldownTickMode(Object.values(tracks), nowMs, visible, reducedMotion);
  useEffect(() => {
    if (mode === "interval") {
      const interval = window.setInterval(() => setNowMs(performance.now()), 1000);
      return () => window.clearInterval(interval);
    }
    if (mode !== "raf" || (typeof document !== "undefined" && document.visibilityState !== "visible")) return;
    let frame = 0;
    const tick = (time: number) => {
      if (typeof document !== "undefined" && document.visibilityState !== "visible") return;
      setNowMs(time);
      frame = window.requestAnimationFrame(tick);
    };
    frame = window.requestAnimationFrame(tick);
    return () => window.cancelAnimationFrame(frame);
  }, [mode]);

  return { tracks, nowMs, reducedMotion, contextKey };
}

function cooldownStyle(fraction: number): CSSProperties {
  return {
    "--cooldown-angle": `${Math.max(0, Math.min(1, fraction)) * 360}deg`,
    "--cooldown-fraction": String(Math.max(0, Math.min(1, fraction))),
  } as CSSProperties;
}

function CooldownVisual({ state, nowMs, reducedMotion }: { state?: CooldownState; nowMs: number; reducedMotion: boolean }) {
  if (!state?.active) return null;
  const view = estimateCooldown(state, nowMs);
  return <span className={`deck-cooldown-visual${view.stale ? " is-stale" : ""}`} style={cooldownStyle(view.fraction)} aria-hidden="true">
    {!reducedMotion && <span className="deck-cooldown-sweep" />}
    <b className="deck-cooldown-number">{view.display}</b>
  </span>;
}

function CooldownBar({ state, nowMs }: { state?: CooldownState; nowMs: number }) {
  if (!state?.active) return null;
  const view = estimateCooldown(state, nowMs);
  return <span className={`deck-cooldown-bar${view.stale ? " is-stale" : ""}`} style={cooldownStyle(view.fraction)} aria-hidden="true">
    <span />
  </span>;
}

function itemPlace(slot: ItemTile["slot"], locale: "th" | "en") {
  if (slot.startsWith("inventory")) {
    const number = Number(slot.slice("inventory".length)) + 1;
    return locale === "th" ? `ช่องหลัก ${number}` : `Main slot ${number}`;
  }
  if (slot.startsWith("backpack")) {
    const number = Number(slot.slice("backpack".length)) + 1;
    return locale === "th" ? `ช่องเป้ B${number}` : `Backpack slot B${number}`;
  }
  return slot === "neutral" ? (locale === "th" ? "ช่องไอเท็มกลาง" : "Neutral item slot")
    : (locale === "th" ? "ช่องวาร์ป" : "Teleport slot");
}

function itemDescription(tile: ItemTile, locale: "th" | "en") {
  const place = itemPlace(tile.slot, locale);
  if (tile.kind === "unknown") return `${place}: ${locale === "th" ? "ไม่ทราบ" : "Unknown"}`;
  if (tile.kind === "empty") return `${place}: ${locale === "th" ? "ว่าง" : "Empty"}`;
  return `${place}: ${tile.name ?? "—"}${tile.rawId ? ` (${tile.rawId})` : ""}`;
}

/** Small round/square slots cannot hold a full name; show initials there and keep the full name in the title. */
function shortItemName(name: string | null): string {
  if (!name) return "—";
  const words = name.split(/\s+/).filter(word => /^[A-Za-z0-9]/.test(word));
  return words.length >= 2 ? words.slice(0, 2).map(word => word[0].toUpperCase()).join("") : name.slice(0, 3);
}

function ItemSlotTile({ tile, locale, shape, label, cooldown, nowMs, reducedMotion, stale }: {
  tile: ItemTile; locale: "th" | "en"; shape: "main" | "backpack" | "teleport" | "neutral";
  label?: string; cooldown?: CooldownState; nowMs: number; reducedMotion: boolean; stale: boolean;
}) {
  const title = `${itemDescription(tile, locale)}${stale ? ` · ${locale === "th" ? "ข้อมูลเก่า" : "Old data"}` : ""}`;
  return <div className={`deck-item-frame${shape === "backpack" ? " is-backpack" : ""}${stale ? " is-stale" : ""}`}>
    <div className={`deck-item-tile is-${tile.kind} is-${shape}${stale ? " is-stale" : ""}`} title={title} aria-label={title}>
      {tile.kind === "unknown" && <span className="deck-item-unknown" aria-hidden="true">?</span>}
      {tile.kind === "item" && <span className="deck-item-name">
        {shape === "teleport" ? "TP" : shape === "neutral" ? shortItemName(tile.name) : tile.name ?? "—"}</span>}
      <CooldownVisual state={cooldown} nowMs={nowMs} reducedMotion={reducedMotion} />
      {tile.charges !== null && tile.kind === "item" && <b className="deck-item-charges" title={`${locale === "th" ? "จำนวนชาร์จ" : "Charges"}: ${tile.charges}`}>×{tile.charges}</b>}
    </div>
    <CooldownBar state={cooldown} nowMs={nowMs} />
    {label && <small className="deck-backpack-label">{label}</small>}
  </div>;
}

function abilityDescription(tile: AbilityTile, locale: "th" | "en", stale: boolean, cooldown: CooldownState | undefined, nowMs: number) {
  const unknown = locale === "th" ? "ไม่ทราบสถานะการใช้" : "Castability unknown";
  const level = `${locale === "th" ? "เลเวล" : "Level"} ${tile.level ?? "—"}`;
  const details = [level];
  if (tile.level === 0) details.push(locale === "th" ? "ยังไม่เรียน" : "Not learned");
  if (stale) {
    details.push(locale === "th" ? "ข้อมูลเก่า" : "Old data");
    if (cooldown?.active) {
      const estimate = estimateCooldown(cooldown, nowMs);
      details.push(`${locale === "th" ? "คูลดาวน์เดิม" : "Last cooldown"} ${estimate.display}s`);
    } else details.push(unknown);
  } else if (tile.canCast === null) details.push(unknown);
  else if (tile.status === "cooldown") {
    const remaining = cooldown?.active ? estimateCooldown(cooldown, nowMs).display : formatCooldownValue(tile.cooldown ?? 0);
    details.push(`${locale === "th" ? "คูลดาวน์" : "Cooldown"} ${remaining}s`);
  } else if (tile.canCast === true && tile.level !== 0) details.push(abilityStatusText(tile, locale));
  else if (tile.canCast === false) details.push(locale === "th" ? "ยังใช้ไม่ได้" : "Not ready");
  return `${tile.name} (${tile.rawId}) · ${details.join(" · ")}`;
}

function AbilitySlotTile({ tile, locale, extra, cooldown, nowMs, reducedMotion, stale }: {
  tile: AbilityTile; locale: "th" | "en"; extra?: boolean; cooldown?: CooldownState; nowMs: number;
  reducedMotion: boolean; stale: boolean;
}) {
  const title = abilityDescription(tile, locale, stale, cooldown, nowMs);
  const ready = !stale && tile.status === "ready" && tile.level !== 0;
  return <article className={`deck-ability-tile${extra ? " is-extra" : ""}${tile.level === 0 ? " is-unlearned" : ""}${stale ? " is-stale" : ""}`}>
    <div className={`deck-ability-slot${ready ? " is-ready" : ""}${tile.status === "cooldown" ? " is-cooldown" : ""}${tile.canCast === null || stale ? " is-unknown" : ""}${tile.ultimate ? " is-ultimate" : ""}${stale ? " is-stale" : ""}`}
      title={title} aria-label={title}>
      {(tile.canCast === null || stale) && <span className="deck-ability-unknown" aria-hidden="true">?</span>}
      {tile.ultimate && <b className="deck-ultimate-tag" aria-hidden="true">ULT</b>}
      <CooldownVisual state={cooldown} nowMs={nowMs} reducedMotion={reducedMotion} />
    </div>
    <CooldownBar state={cooldown} nowMs={nowMs} />
    <span className="deck-level-pips" aria-label={`${locale === "th" ? "เลเวล" : "Level"}: ${tile.level ?? "—"}`}>
      {tile.pips ? tile.pips.map((learned, index) => <i key={index} className={learned ? "is-learned" : undefined} aria-hidden="true" />) : "—"}
    </span>
    <strong className="deck-ability-name" title={title}>{tile.name}</strong>
  </article>;
}

export function OwnLoadout({ model, locale, gsiSetup }: {
  model: LoadoutViewModel;
  locale: "th" | "en";
  gsiSetup?: (replace: boolean) => Promise<string | null>;
}) {
  const th = locale === "th";
  const [setupState, setSetupState] = useState<"idle" | "busy" | "done" | string>("idle");
  const { tracks, nowMs, reducedMotion, contextKey } = useCooldownClock(model);
  const [freshCache, setFreshCache] = useState<FreshLoadoutCache | null>(null);
  const presentationKey = JSON.stringify([contextKey, model.source.sequence, model.items.stale, model.items.slots,
    model.abilities.stale, model.abilities.tiles]);
  useEffect(() => {
    setFreshCache(previous => retainFreshLoadout(model, previous).cache);
  }, [presentationKey]);
  const runSetup = async () => {
    if (!gsiSetup) return;
    setSetupState("busy");
    const error = await gsiSetup(false);
    setSetupState(error === null ? "done" : `error:${error}`);
  };
  const old = (age: number | null) => `${th ? "ข้อมูลเก่า" : "Old data"} · ${seconds(age, locale)}`;
  const presentation = retainFreshLoadout(model, freshCache);
  const itemGroups = presentation.items ? groupItemSlots(presentation.items) : null;
  const abilityRows = presentation.abilities?.length ? splitAbilityRows(presentation.abilities) : null;

  return <section className="deck-loadout" aria-label={th ? "ไอเท็มและสกิล" : "Items and abilities"}>
    <section className={`deck-loadout-section${model.items.stale ? " is-stale" : ""}`} aria-label={th ? "ไอเท็ม" : "Items"}>
      <header><h2>{th ? "ไอเท็ม" : "Items"}</h2>{model.items.stale && <small>{old(model.items.ageMs)}</small>}</header>
      {itemGroups ? <>
        <div className="deck-inventory-layout">
          <div className="deck-inventory" aria-label={th ? "ช่องหลัก 6 ช่อง" : "Six main item slots"}>
            {itemGroups.main.map(tile => <ItemSlotTile key={tile.slot} tile={tile} locale={locale} shape="main"
              cooldown={tracks[cooldownTrackKey("item", contextKey, tile.slot, tile.rawId)]} nowMs={nowMs}
              reducedMotion={reducedMotion} stale={model.items.stale} />)}
          </div>
          <div className="deck-special-items">
            {itemGroups.neutral && <ItemSlotTile tile={itemGroups.neutral} locale={locale} shape="neutral"
              cooldown={tracks[cooldownTrackKey("item", contextKey, itemGroups.neutral.slot, itemGroups.neutral.rawId)]} nowMs={nowMs}
              reducedMotion={reducedMotion} stale={model.items.stale} />}
            {itemGroups.teleport && <ItemSlotTile tile={itemGroups.teleport} locale={locale} shape="teleport"
              cooldown={tracks[cooldownTrackKey("item", contextKey, itemGroups.teleport.slot, itemGroups.teleport.rawId)]} nowMs={nowMs}
              reducedMotion={reducedMotion} stale={model.items.stale} />}
          </div>
        </div>
        <div className="deck-backpack" aria-label={th ? "ช่องเป้ 3 ช่อง" : "Three backpack slots"}>
          {itemGroups.backpack.map((tile, index) => <ItemSlotTile key={tile.slot} tile={tile} locale={locale} shape="backpack"
            label={`B${index + 1}`} cooldown={tracks[cooldownTrackKey("item", contextKey, tile.slot, tile.rawId)]} nowMs={nowMs}
            reducedMotion={reducedMotion} stale={model.items.stale} />)}
        </div>
      </> : <span className="deck-loadout-dash">—</span>}
    </section>
    <section className={`deck-loadout-section${model.abilities.stale ? " is-stale" : ""}`} aria-label={th ? "สกิลและคูลดาวน์" : "Abilities and cooldowns"}>
      <header><h2>{th ? "สกิล · คูลดาวน์" : "Abilities · cooldowns"}</h2>{model.abilities.stale && <small>{old(model.abilities.ageMs)}</small>}</header>
      {abilityRows ? <div className="deck-ability-groups">
        {abilityRows.hero.length > 0 && <div className="deck-ability-grid" aria-label={th ? "สกิลฮีโร่" : "Hero abilities"}>
          {abilityRows.hero.map(tile => <AbilitySlotTile key={tile.slot} tile={tile} locale={locale}
            cooldown={tracks[cooldownTrackKey("ability", contextKey, tile.slot, tile.rawId)]} nowMs={nowMs}
            reducedMotion={reducedMotion} stale={model.abilities.stale} />)}
        </div>}
        {abilityRows.extras.length > 0 && <section className="deck-extra-abilities" aria-label={th ? "สกิลเสริม" : "Extras"}>
          <h3>{th ? "สกิลเสริม" : "Extras"}</h3>
          <div className="deck-ability-grid is-extras">
            {abilityRows.extras.map(tile => <AbilitySlotTile key={tile.slot} tile={tile} locale={locale} extra
              cooldown={tracks[cooldownTrackKey("ability", contextKey, tile.slot, tile.rawId)]} nowMs={nowMs}
              reducedMotion={reducedMotion} stale={model.abilities.stale} />)}
          </div>
        </section>}
      </div> : <span className="deck-loadout-dash">—</span>}
    </section>
    {model.setupHint && <small className="deck-stats-caption deck-setup-hint">
      <span>{th ? "ต้องตั้งค่าการเชื่อมต่อ Dota 2 ใหม่หนึ่งครั้ง" : "Run Dota 2 connection setup once again"}</span>
      <span>{th ? "เกมที่เปิดก่อนเพิ่มบล็อก items/abilities ใน cfg จะยังไม่ส่งข้อมูลส่วนนี้" : "A game started before the cfg gained items/abilities blocks will not send them yet."}</span>
      {gsiSetup && <button type="button" className="deck-setup-button" disabled={setupState === "busy"} onClick={() => void runSetup()}>
        {setupState === "busy" ? (th ? "กำลังติดตั้ง…" : "Installing…") : (th ? "ตั้งค่า GSI / ติดตั้งไฟล์เชื่อมต่อ Dota 2" : "Set up GSI / install the Dota 2 connection file")}
      </button>}
      {setupState === "done" && <span role="status">{th ? "ติดตั้งแล้ว — ปิดแล้วเปิด Dota 2 ใหม่หนึ่งครั้ง" : "Installed — restart Dota 2 once."}</span>}
      {setupState.startsWith("error:") && <span role="alert">{reason(setupState.slice(6), locale)}</span>}
    </small>}
  </section>;
}
