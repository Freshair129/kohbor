export interface ModifierIconInput {
  modifier: string;
  sourceHeroId?: string | null;
  sourceAbilities?: string[];
  recentCasts?: { time: number; name: string; start: number }[];
  iconExists?: (kind: "ability" | "item", name: string) => boolean;
  overrideTable?: Record<string, { kind: "ability" | "item"; icon: string }>;
  effectKind?: "control" | "debuff" | "buff";
}
export interface ModifierIcon {
  kind: "ability" | "item" | "glyph";
  icon: string | null;
  glyph: string | null;
  tier: 0 | 1 | 2 | 3 | 4 | 5 | 6;
  inferred: boolean;
  sourceHeroId: string | null;
}
export function resolveModifierIcon(input: ModifierIconInput): ModifierIcon;
export function statusIconTitle(input: {
  name: string; kind?: "control" | "debuff" | "buff"; sourceNames?: string[]; remaining?: number; permanent?: boolean; inferred?: boolean; locale?: "th" | "en";
}): string;
