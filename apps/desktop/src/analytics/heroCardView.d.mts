export type HeroCardRowId = "top" | "bars" | "ab" | "it" | "st" | "dv";
export type HeroCardRowView = { id: HeroCardRowId; values: Record<string, unknown>; unknownFields: string[] };
export type HeroCardViewModel = { rows: [HeroCardRowView, HeroCardRowView, HeroCardRowView, HeroCardRowView, HeroCardRowView, HeroCardRowView] };
export const HERO_CARD_ROW_IDS: readonly HeroCardRowId[];
export function heroCardView(input?: Partial<Record<HeroCardRowId, Record<string, unknown>>>): HeroCardViewModel;
