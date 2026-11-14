// @ts-ignore Node's built-in TypeScript loader requires the explicit source extension.
import type { OwnSnapshot } from "../types.ts";
// @ts-ignore Explicit extensions support Node's TypeScript loader.
import { OWN_STAT_FIELDS, type OwnStatKey } from "../ownSnapshot.ts";
// @ts-ignore Explicit extensions support Node's TypeScript loader.
import { displayHeroName, heroNameCaption, type HeroName } from "./heroName.ts";

function freshValue<T>(field: { value: T | null; availability: string } | undefined): T | null {
  return field?.availability === "fresh" && field.value !== null && field.value !== undefined ? field.value : null;
}

function finiteNumber(field: { value: number | null; availability: string } | undefined): number | null {
  const value = freshValue(field);
  return typeof value === "number" && Number.isFinite(value) ? value : null;
}

export interface OwnHeroViewModel {
  snapshotAvailability: OwnSnapshot["availability"] | null;
  heroId: string | null;
  heroName: HeroName | null;
  heroCaption: string | null;
  level: number | null;
  hp: number | null;
  maxHp: number | null;
  hpPercent: number | null;
  manaPercent: number | null;
  alive: boolean | null;
  respawnSeconds: number | null;
  ageMs: number | null;
  hasOwnSnapshot: boolean;
  stats: Record<OwnStatKey, number | null>;
}

/** Keep each displayed value tied to that field's own freshness marker. */
export function deriveOwnHeroViewModel(snapshot: OwnSnapshot | null): OwnHeroViewModel {
  const fields = snapshot?.own?.fields;
  const heroId = freshValue(fields?.hero_id);
  const hp = finiteNumber(fields?.hp);
  const maxHp = finiteNumber(fields?.max_hp);
  const sourceAge = snapshot?.source.age_ms;
  const ageAvailable = snapshot?.availability === "fresh" || snapshot?.availability === "stale";
  const ageMs = ageAvailable && typeof sourceAge === "number" && Number.isFinite(sourceAge) && sourceAge >= 0
    ? sourceAge
    : null;

  return {
    snapshotAvailability: snapshot?.availability ?? null,
    heroId,
    heroName: displayHeroName(heroId),
    heroCaption: heroNameCaption(heroId),
    level: finiteNumber(fields?.level),
    hp,
    maxHp,
    hpPercent: hp !== null && maxHp !== null && maxHp > 0 ? (hp / maxHp) * 100 : null,
    manaPercent: finiteNumber(fields?.mana_percent),
    alive: freshValue(fields?.alive),
    respawnSeconds: finiteNumber(fields?.respawn_seconds),
    ageMs,
    hasOwnSnapshot: snapshot?.own !== null && snapshot?.own !== undefined,
    stats: Object.fromEntries(OWN_STAT_FIELDS.map(name => [name, finiteNumber(fields?.[name])])) as Record<OwnStatKey, number | null>,
  };
}

export function ownHeroLifeLabel(model: OwnHeroViewModel, locale: "th" | "en"): string {
  if (model.alive === null) return "—";
  if (model.alive) return locale === "th" ? "มีชีวิต" : "Alive";
  const respawn = model.respawnSeconds === null ? "—" : String(model.respawnSeconds);
  return locale === "th" ? `เกิดใหม่ใน ${respawn} วินาที` : `Respawns in ${respawn}s`;
}
