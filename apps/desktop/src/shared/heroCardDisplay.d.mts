export type HeroCardGroup = "attrs" | "dmg" | "reg" | "arm" | "mr" | "amp" | "as" | "ms" | "ev" | "sr" | "ls" | "rates" | "status";
export type HeroCardStatKey = "str" | "agi" | "int" | "dmg" | "reg" | "arm" | "mr" | "amp" | "as" | "ms" | "ev" | "sr" | "ls" | "gpm" | "xpm";
export interface HeroCardDisplay { v: 1; show: Record<HeroCardGroup, boolean> }
export const HERO_CARD_DISPLAY_KEY: "kohbor.hero-card.display";
export const HERO_CARD_DISPLAY_EVENT: "kohbor:hero-card-display-change";
export const HERO_CARD_GROUPS: readonly HeroCardGroup[];
export const HERO_CARD_GROUP_KEYS: Readonly<Record<Exclude<HeroCardGroup, "status">, readonly HeroCardStatKey[]>>;
export const HERO_CARD_GROUP_LABELS: Readonly<Record<HeroCardGroup, string>>;
export const DEFAULT_HERO_CARD_DISPLAY: HeroCardDisplay;
export const CORE_HERO_CARD_DISPLAY: HeroCardDisplay;
export function normalizeHeroCardDisplay(value: unknown): HeroCardDisplay;
export function readHeroCardDisplay(storage: Pick<Storage, "getItem"> | undefined): HeroCardDisplay;
export function writeHeroCardDisplay(storage: Pick<Storage, "setItem"> | undefined, value: unknown): HeroCardDisplay;
export function visibleStatKeys(display: unknown, options?: { includeLiveOnly?: boolean }): ReadonlySet<HeroCardStatKey>;
export function reflowHeroCardDisplay(display: unknown, density?: "full" | "compact"): {
  visibleStats: HeroCardStatKey[]; showStatus: boolean; emptyRows: string[]; derivedColumns: number; placeholderCount: 0;
};
export function visibleHeroCardRows(display: unknown, density?: "full" | "compact" | "roster", selected?: boolean, includeLiveOnly?: boolean): string[];
export function setHeroCardDisplayGroup(display: unknown, group: HeroCardGroup, visible: boolean): HeroCardDisplay;
