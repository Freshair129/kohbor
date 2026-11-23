// View model for the replay-style hero cards on the centred Live deck (DEC-REPLAY-INTEGRATION item 10, slice R5;
// amended by the owner 2026-11-24: every card shows the replay card's full structure).
// Every card (own hero and the nine others) has the same sections as the replay card: portrait with level, name,
// K/D/A, LH/DN, gold, HP and mana bars, ability row with buff chips, six items + backpack + TP + neutral, and the
// attribute and derived stat rows. The own hero fills what GSI sends; the nine others reuse the second-monitor card
// model (secondMonitorCards.ts), so no new data path is added.
// Rules: unknown is a real state (null, rendered "—" or an empty dashed slot), never 0; no stat is computed on Live
// (GSI sends no attributes), so every stat cell is "—" until a source exists; no path traces; no enemy value that is
// not observed; a missing icon falls back to the runtime portrait or a name tile.
// @ts-ignore Node's built-in TypeScript loader requires the explicit source extension.
import type { OwnHeroViewModel } from "../deck/ownHeroModel.ts";
// @ts-ignore Node's built-in TypeScript loader requires the explicit source extension.
import { groupItemSlots, splitAbilityRows, type AbilityTile, type ItemTile } from "../deck/loadoutModel.ts";
// @ts-ignore Node's built-in TypeScript loader requires the explicit source extension.
import { formatCooldownValue } from "../deck/cooldownModel.ts";
// @ts-ignore Node's built-in TypeScript loader requires the explicit source extension.
import type { HeroCard, SkillState, SkillSource } from "./secondMonitorCards.ts";
// @ts-ignore Node's built-in TypeScript loader requires the explicit source extension.
import type { IconResolver } from "./dotaIcons.ts";

export const UNKNOWN = "—";

/** In-game player slot colours (top bar order). Used only when the draft slot is known; otherwise the team token. */
export const SLOT_COLORS = ["#3375ff", "#66ffbf", "#bf00bf", "#f3f00b", "#ff6b00", "#fe86c2", "#a1b447", "#65d9f7", "#008321", "#a46900"] as const;

export function slotColor(slot: number | null | undefined): string | null {
  return typeof slot === "number" && Number.isInteger(slot) ? SLOT_COLORS[slot] ?? null : null;
}

const num = (value: number | null | undefined): string => typeof value === "number" && Number.isFinite(value) ? String(value) : UNKNOWN;
/** "a / b / c" when every part is known, otherwise each unknown part is an em dash. All unknown collapses to one dash. */
export function joinKnown(values: (number | null)[], separator = "/"): string {
  return values.every(value => value === null) ? UNKNOWN : values.map(num).join(separator);
}
/** Gold as the replay card prints net worth: 9,850 or 12.3k. */
export function formatGold(value: number | null): string {
  if (value === null || !Number.isFinite(value)) return UNKNOWN;
  return value >= 10000 ? `${(value / 1000).toFixed(1)}k` : Math.round(value).toLocaleString("en-US");
}
/** Short label for an icon tile without an image: initials of a multi-word name, else its first three letters. */
export function tileText(name: string | null): string {
  if (!name) return UNKNOWN;
  const words = name.split(/[\s_-]+/).filter(word => /^[\p{L}\p{N}]/u.test(word));
  return words.length >= 2 ? words.slice(0, 2).map(word => word.charAt(0).toUpperCase()).join("") : name.slice(0, 3);
}

export type IconState = "filled" | "empty" | "unknown";
export type CastState = "ready" | "cooldown" | "unknown" | "not_learned";
export interface IconSlotView {
  key: string;
  state: IconState;
  name: string | null;
  rawId: string | null;
  src: string | null;
  text: string;
  cooldown: number | null;
  cooldownText: string | null;
  charges: number | null;
  /** Ability level and pip count (4, or 6 for an ultimate); null for items or when unknown. */
  level: number | null;
  maxLevel: number | null;
  ultimate: boolean;
  cast: CastState;
}

/** A slot with no admitted source: drawn as an empty dashed slot, never as an empty item or a real one. */
export function unknownSlot(key: string, ultimate = false): IconSlotView {
  return { key, state: "unknown", name: null, rawId: null, src: null, text: "", cooldown: null, cooldownText: null, charges: null,
    level: null, maxLevel: null, ultimate, cast: "unknown" };
}

function itemSlot(tile: ItemTile | null, icons: IconResolver, key: string): IconSlotView {
  if (!tile || tile.kind !== "item") return tile?.kind === "empty" ? { ...unknownSlot(key), state: "empty" } : unknownSlot(key);
  const cooldown = tile.cooldown;
  return { key, state: "filled", name: tile.name, rawId: tile.rawId, src: icons("item", tile.rawId), text: tileText(tile.name),
    cooldown, cooldownText: cooldown === null ? null : formatCooldownValue(cooldown), charges: tile.charges,
    level: null, maxLevel: null, ultimate: false, cast: cooldown !== null ? "cooldown" : "unknown" };
}

function abilitySlot(tile: AbilityTile, icons: IconResolver, stale: boolean): IconSlotView {
  const cooldown = stale ? null : tile.cooldown;
  const cast: CastState = tile.level === 0 ? "not_learned" : stale ? "unknown"
    : tile.status === "cooldown" && cooldown !== null ? "cooldown" : tile.status === "ready" ? "ready" : "unknown";
  return { key: `a${tile.slot}`, state: "filled", name: tile.name, rawId: tile.rawId, src: icons("ability", tile.rawId), text: tileText(tile.name),
    cooldown: cast === "cooldown" ? cooldown : null, cooldownText: cast === "cooldown" && cooldown !== null ? formatCooldownValue(cooldown) : null,
    charges: null, level: tile.level, maxLevel: tile.pips?.length ?? null, ultimate: tile.ultimate === true, cast };
}

/** Three regular abilities and the ultimate, all unknown: the replay card's ability row with no source. */
export function unknownAbilities(): IconSlotView[] {
  return [unknownSlot("a0"), unknownSlot("a1"), unknownSlot("a2"), unknownSlot("a3", true)];
}

export interface ItemsView { main: IconSlotView[]; backpack: IconSlotView[]; teleport: IconSlotView; neutral: IconSlotView; stale: boolean; known: boolean }
export interface AbilitiesView { hero: IconSlotView[]; extras: IconSlotView[]; stale: boolean; known: boolean }

/** Six main slots, three backpack slots, TP and neutral, all unknown. */
export function unknownItems(): ItemsView {
  return {
    main: [0, 1, 2, 3, 4, 5].map(n => unknownSlot(`inventory${n}`)),
    backpack: [0, 1, 2].map(n => unknownSlot(`backpack${n}`)),
    teleport: unknownSlot("teleport"), neutral: unknownSlot("neutral"), stale: false, known: false,
  };
}

/**
 * The replay card's attribute and derived stat cells, in its order: STR/AGI/INT (with gains), DMG, REG, then
 * ARM, MR, AMP, AS, MS, EV, SR, LS; GPM/XPM follow for the own hero's GSI rates. Live has no source for the attribute
 * and derived values (GSI sends none and Live computes nothing), so those cells are always "—" in this slice.
 */
export type StatKey = "str" | "agi" | "int" | "dmg" | "reg" | "arm" | "mr" | "amp" | "as" | "ms" | "ev" | "sr" | "ls" | "gpm" | "xpm";
export interface StatCellView { key: StatKey; label: string; value: string; gain: string | null; group: "attribute" | "derived" | "rate" }
const STAT_CELLS: readonly [StatKey, string, StatCellView["group"]][] = [
  ["str", "STR", "attribute"], ["agi", "AGI", "attribute"], ["int", "INT", "attribute"], ["dmg", "DMG", "attribute"], ["reg", "REG", "attribute"],
  ["arm", "ARM", "derived"], ["mr", "MR", "derived"], ["amp", "AMP", "derived"], ["as", "AS", "derived"],
  ["ms", "MS", "derived"], ["ev", "EV", "derived"], ["sr", "SR", "derived"], ["ls", "LS", "derived"],
  ["gpm", "GPM", "rate"], ["xpm", "XPM", "rate"],
];
export function statCells(rates: { gpm: number | null; xpm: number | null } = { gpm: null, xpm: null }): StatCellView[] {
  return STAT_CELLS.map(([key, label, group]) => ({ key, label, group, gain: null,
    value: key === "gpm" ? num(rates.gpm) : key === "xpm" ? num(rates.xpm) : UNKNOWN }));
}

export interface FullHeroCardView {
  variant: "full";
  key: string;
  side: "own" | "ally" | "enemy";
  slot: number | null;
  color: string | null;
  heroId: string | null;
  heroName: string | null;
  heroCaption: string | null;
  portraitSrc: string | null;
  levelText: string;
  kdaText: string;
  lhdnText: string;
  gold: number | null;
  goldText: string;
  ratesText: string;
  hpPercent: number | null;
  hpText: string;
  hpLow: boolean;
  manaPercent: number | null;
  manaText: string;
  alive: boolean | null;
  respawnText: string | null;
  stale: boolean;
  /** Always present: unknown slots when there is no source, so the card never loses a section. */
  abilities: AbilitiesView;
  /** Buff chips: Live has no buff source yet, so this stays empty. */
  buffs: string[];
  items: ItemsView;
  stats: StatCellView[];
  /** Ally ultimate dot from an admitted source (peer or the top-bar read); null otherwise and always for enemies. */
  ult: { state: SkillState; source: Exclude<SkillSource, null>; seconds: number | null } | null;
}

const clampPercent = (value: number | null): number | null =>
  value === null || !Number.isFinite(value) ? null : Math.max(0, Math.min(100, value));
const percentText = (value: number | null): string => value === null ? UNKNOWN : `${Math.round(value)}%`;

/**
 * Own hero, full card, from GSI only. Items and abilities are the tiles the caller chose to present (fresh, or the
 * last fresh value of the same context while stale); null means no data yet and renders unknown slots. GSI sends no
 * attributes or derived stats, so those cells are "—".
 */
export function ownFullCard(input: {
  own: OwnHeroViewModel;
  items: ItemTile[] | null; itemsStale: boolean;
  abilities: AbilityTile[] | null; abilitiesStale: boolean;
  slot?: number | null; stale?: boolean; icons: IconResolver;
}): FullHeroCardView {
  const { own, icons } = input;
  const s = own.stats;
  const hpPercent = clampPercent(own.hpPercent);
  const manaPercent = clampPercent(own.manaPercent);
  const groups = input.items ? groupItemSlots(input.items) : null;
  const rows = input.abilities && input.abilities.length > 0 ? splitAbilityRows(input.abilities) : null;
  const slot = input.slot ?? null;
  return {
    variant: "full", key: "own", side: "own", slot, color: slotColor(slot),
    heroId: own.heroName ? own.heroId : null, heroName: own.heroName?.name ?? null, heroCaption: own.heroCaption,
    portraitSrc: own.heroName ? icons("hero", own.heroId) : null,
    levelText: num(own.level),
    kdaText: joinKnown([s.kills, s.deaths, s.assists]),
    lhdnText: joinKnown([s.last_hits, s.denies]),
    gold: s.gold, goldText: formatGold(s.gold),
    ratesText: joinKnown([s.gpm, s.xpm], " / "),
    hpPercent, hpText: own.hp === null && own.maxHp === null ? UNKNOWN : `${num(own.hp)} / ${num(own.maxHp)}`,
    hpLow: hpPercent !== null && hpPercent < 25,
    manaPercent, manaText: percentText(manaPercent),
    alive: own.alive,
    respawnText: own.alive === false && own.respawnSeconds !== null ? String(own.respawnSeconds) : null,
    stale: input.stale === true,
    abilities: rows ? {
      hero: rows.hero.map(tile => abilitySlot(tile, icons, input.abilitiesStale)),
      extras: rows.extras.map(tile => abilitySlot(tile, icons, input.abilitiesStale)),
      stale: input.abilitiesStale, known: true,
    } : { hero: unknownAbilities(), extras: [], stale: false, known: false },
    buffs: [],
    items: groups ? {
      main: groups.main.map(tile => itemSlot(tile, icons, tile.slot)),
      backpack: groups.backpack.map(tile => itemSlot(tile, icons, tile.slot)),
      teleport: itemSlot(groups.teleport, icons, "teleport"),
      neutral: itemSlot(groups.neutral, icons, "neutral"),
      stale: input.itemsStale, known: true,
    } : unknownItems(),
    stats: statCells({ gpm: s.gpm, xpm: s.xpm }),
    ult: null,
  };
}

export interface CompactHeroCardView {
  variant: "compact";
  key: string;
  side: "ally" | "enemy";
  slot: number | null;
  color: string | null;
  heroId: string | null;
  heroName: string | null;
  nameSource: "draft" | "gsi" | null;
  portraitSrc: string | null;
  /** Shown only with an admitted source (consented peer sharing for allies); never for enemies on this slice. */
  hpPercent: number | null;
  manaPercent: number | null;
  vitalsSource: "gsi" | "peer" | null;
  /** Ultimate square: only when a source exists (peer or the top-bar dot). */
  ult: { state: SkillState; source: Exclude<SkillSource, null>; seconds: number | null } | null;
}

/** Compact card for an ally or enemy from the second-monitor card model. Enemy vitals and skills are always unknown. */
export function compactCard(card: HeroCard, icons: IconResolver): CompactHeroCardView {
  const side = card.side === "enemy" ? "enemy" : "ally";
  const enemy = side === "enemy";
  const ult = card.skills[3];
  const named = card.heroName !== null && card.heroId !== null;
  return {
    variant: "compact", key: card.key, side, slot: card.slot, color: slotColor(card.slot),
    heroId: named ? card.heroId : null, heroName: named ? card.heroName : null, nameSource: named ? card.nameSource : null,
    portraitSrc: named ? icons("hero", card.heroId) : null,
    hpPercent: enemy ? null : clampPercent(card.hpPercent),
    manaPercent: enemy ? null : clampPercent(card.manaPercent),
    vitalsSource: enemy ? null : card.vitalsSource,
    ult: !enemy && ult.source !== null && ult.state !== "unknown" ? { state: ult.state, source: ult.source, seconds: ult.seconds } : null,
  };
}

/** A peer-shared skill shown in an ally's ability slot: ready, or cooldown with seconds; anything else stays unknown. */
function sharedSkill(square: HeroCard["skills"][number], key: string): IconSlotView {
  const slot = unknownSlot(key, square.ultimate);
  if (square.source !== "peer") return slot;
  if (square.state === "cooldown" && square.seconds !== null) {
    return { ...slot, cast: "cooldown", cooldown: square.seconds, cooldownText: formatCooldownValue(square.seconds) };
  }
  return square.state === "ready" ? { ...slot, cast: "ready" } : slot;
}

/**
 * An ally or enemy card with the replay card's full structure. Identity, slot colour and the admitted ally values
 * come from compactCard (so enemy values stay null even if a card arrives with them); every other section is shown
 * with "—" or unknown slots. Ally values beyond the draft name need teammate sharing, which does not exist yet.
 */
export function teamFullCard(card: HeroCard, icons: IconResolver): FullHeroCardView {
  const view = compactCard(card, icons);
  const enemy = view.side === "enemy";
  const abilities = enemy ? unknownAbilities() : card.skills.map((square, index) => sharedSkill(square, `a${index}`));
  return {
    variant: "full", key: view.key, side: view.side, slot: view.slot, color: view.color,
    heroId: view.heroId, heroName: view.heroName, heroCaption: null, portraitSrc: view.portraitSrc,
    levelText: UNKNOWN, kdaText: UNKNOWN, lhdnText: UNKNOWN, gold: null, goldText: UNKNOWN, ratesText: UNKNOWN,
    hpPercent: view.hpPercent, hpText: percentText(view.hpPercent), hpLow: false,
    manaPercent: view.manaPercent, manaText: percentText(view.manaPercent),
    alive: null, respawnText: null, stale: false,
    abilities: { hero: abilities, extras: [], stale: false, known: false },
    buffs: [],
    items: unknownItems(),
    stats: statCells(),
    ult: view.ult,
  };
}
