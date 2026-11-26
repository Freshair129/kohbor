export type HeroCardRowId = "top" | "bars" | "ab" | "it" | "st" | "dv";
export type HeroCardRowView = { id: HeroCardRowId; values: Record<string, unknown>; unknownFields: string[] };
export type HeroDamageCell = { label: "HD"; value: string; unknown: boolean };
export type HeroCardViewModel = { rows: [HeroCardRowView, HeroCardRowView, HeroCardRowView, HeroCardRowView, HeroCardRowView, HeroCardRowView]; hd: HeroDamageCell | null };
export const HERO_CARD_ROW_IDS: readonly HeroCardRowId[];
export function heroCardView(input?: Partial<Record<HeroCardRowId, Record<string, unknown>>>): HeroCardViewModel;
export function heroDamageCell(value: unknown): HeroDamageCell;
