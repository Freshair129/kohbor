export interface ReplayPatchData {
  schema_version: 1;
  patch: string;
  source: string;
  map: {
    size: number;
    image: string;
    calibration: { px: number[]; py: number[]; [key: string]: unknown };
  };
  base_attack_speed: Record<string, number>;
  hero_names: Record<string, string>;
  rules: Record<string, unknown> & {
    mr_per_int_pct?: number;
    armor_per_agi?: number;
  };
  formulas: Record<string, unknown> & {
    magic_resistance_per_intelligence_pct?: { value?: number; [key: string]: unknown };
    armor_per_agility?: { value?: number; [key: string]: unknown };
  };
  buffs: Record<string, Record<string, unknown>>;
  ability_names_by_hero: Record<string, string[]>;
  control_abilities_by_hero: Record<string, string[]>;
  control_ability_source: Record<string, { dname: string | null; desc: string }>;
  control_terms: string[];
  mobility_abilities_by_hero: Record<string, string[]>;
  mobility_items: string[];
  consumable_items: string[];
  notable_items: string[];
  ultimate_classification: {
    available: boolean;
    reason: string;
    abilities_by_hero: Record<string, string[]> | null;
  };
}

export type ReplayPackLookup =
  | { state: "ready"; pack: ReplayPatchData }
  | { state: "needs_pack"; patch: string | null };
export type MapCalibration = { size: number; a: number; b: number; c: number; d: number };
export function replayPackForPatch(patch: string | null | undefined): ReplayPackLookup;
export function chooseMapCalibration(
  manifestCalibration: unknown,
  manifestSize: number | null | undefined,
  packMap: ReplayPatchData["map"] | undefined,
): { calibration: MapCalibration | null; source: "manifest" | "pack" | "unknown" };
export function isControlEffect(pack: ReplayPatchData | null, name: string): boolean;
export function isUltimateAbility(pack: ReplayPatchData | null, heroId: string, abilityName: string): boolean;
export function isMobilityAbility(pack: ReplayPatchData | null, heroId: string, name: string): boolean;
export function isMobilityItem(pack: ReplayPatchData | null, name: string): boolean;
export function isConsumableItem(pack: ReplayPatchData | null, name: string): boolean;
export function isNotableItem(pack: ReplayPatchData | null, name: string): boolean;
export function baseAttackSpeed(pack: ReplayPatchData | null, heroId: string): number | null;
